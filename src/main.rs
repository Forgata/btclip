mod bluetooth;
mod clipboard;
mod protocol;

use bluetooth::BluetoothServer;
use clipboard::ClipboardManager;
use protocol::{FrameType, decode_frame, encode_frame};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

// Enforce a strict limit to prevent malicious/corrupt OOM panics
const MAX_PAYLOAD_SIZE: u32 = 20 * 1024 * 1024; // 20 MB Max

#[tokio::main]
async fn main() {
    println!("⚡ btclip Windows Host Engine starting...");

    // 1. Initialize Thread-Safe State
    let clipboard = match ClipboardManager::new() {
        Ok(mgr) => Arc::new(Mutex::new(mgr)),
        Err(e) => {
            eprintln!("❌ Failed to initialize clipboard: {}", e);
            return;
        }
    };

    // 2. Start RFCOMM Server
    let (_server, mut rx) = match BluetoothServer::start().await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("❌ Failed to start Bluetooth server: {}", e);
            return;
        }
    };

    println!("📡 Waiting for phone connection...");

    // 3. Main Connection Loop
    while let Some(connection) = rx.recv().await {
        println!("📱 Phone connected over Bluetooth!");

        let (mut reader, mut writer) = connection.into_split();
        let read_clipboard = Arc::clone(&clipboard);
        let write_clipboard = Arc::clone(&clipboard);

        // FIX: Connection-scoped synchronization flag prevents multi-session data leakage
        let is_internal_write = Arc::new(AtomicBool::new(false));
        let read_flag = Arc::clone(&is_internal_write);
        let write_flag = Arc::clone(&is_internal_write);

        // ------------------------------------------------------------------
        // TASK 1: Read Loop (Phone -> PC)
        // ------------------------------------------------------------------
        let read_handle = tokio::spawn(async move {
            loop {
                // Read 5-byte header safely
                let header_bytes = match reader.read_bytes(5).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("🔌 Phone disconnected (Header Read): {}", e);
                        break;
                    }
                };

                // Decode length safely from header
                let mut len_bytes = [0u8; 4];
                len_bytes.copy_from_slice(&header_bytes[1..5]);
                let payload_len = u32::from_be_bytes(len_bytes);

                // FIX: Protect engine from OOM memory corruption attacks/malformed frames
                if payload_len > MAX_PAYLOAD_SIZE {
                    eprintln!(
                        "❌ Rejected oversized protocol frame: {} bytes",
                        payload_len
                    );
                    break;
                }

                // Read full payload
                let payload_bytes = match reader.read_bytes(payload_len).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("🔌 Phone disconnected (Payload Read): {}", e);
                        break;
                    }
                };

                // Combine into single frame buffer for protocol decoding
                let mut raw_frame = Vec::with_capacity(5 + payload_len as usize);
                raw_frame.extend_from_slice(&header_bytes);
                raw_frame.extend_from_slice(&payload_bytes);

                match decode_frame(&raw_frame) {
                    Ok(frame) => match frame.frame_type {
                        FrameType::Text => {
                            if let Ok(text) = String::from_utf8(frame.payload) {
                                println!("📥 Received Text from Phone: '{}'", text);
                                // Set flag BEFORE mutating clipboard to minimize race windows
                                read_flag.store(true, Ordering::SeqCst);
                                let mut clip = read_clipboard.lock().await;
                                let _ = clip.set_text(&text);
                            }
                        }
                        FrameType::Image => {
                            println!("📥 Received Screenshot from Phone ({} bytes)", frame.length);
                            read_flag.store(true, Ordering::SeqCst);
                            let mut clip = read_clipboard.lock().await;
                            if let Err(e) = clip.set_image_from_bytes(&frame.payload) {
                                eprintln!("❌ Failed to write screenshot to OS: {}", e);
                            } else {
                                println!("📋 Screenshot pasted to PC clipboard!");
                            }
                        }
                    },
                    Err(e) => eprintln!("❌ Invalid protocol frame received: {:?}", e),
                }
            }
        });

        // ------------------------------------------------------------------
        // TASK 2: Write Loop (PC -> Phone)
        // ------------------------------------------------------------------
        let write_handle = tokio::spawn(async move {
            let mut last_copied_text = String::new();

            loop {
                sleep(Duration::from_millis(400)).await;

                // FIX: Guard scope containment. Mutex drops instantly after extracting text.
                let current_text = {
                    let mut clip = write_clipboard.lock().await;
                    clip.get_text().unwrap_or_default()
                };

                // Check if text changed
                if !current_text.is_empty() && current_text != last_copied_text {
                    // Check if this update originated from the remote client phone
                    if write_flag.swap(false, Ordering::SeqCst) {
                        last_copied_text = current_text;
                        continue;
                    }

                    println!("📤 Local PC Text copied: '{}'", current_text);
                    last_copied_text = current_text.clone();

                    let frame = encode_frame(FrameType::Text, current_text.as_bytes());
                    if let Err(e) = writer.write_bytes(&frame).await {
                        eprintln!("🔌 Phone disconnected during write: {}", e);
                        break;
                    }
                }
            }
        });

        // Wait until current session drops
        let _ = tokio::join!(read_handle, write_handle);
        println!("♻️ Returned to listening state for next phone connection...");
    }
}
