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

const MAX_PAYLOAD_SIZE: u32 = 20 * 1024 * 1024; // 20 MB

#[tokio::main]
async fn main() {
    println!("⚡ btclip Windows Host Engine starting...");

    let clipboard = match ClipboardManager::new() {
        Ok(mgr) => Arc::new(Mutex::new(mgr)),
        Err(e) => {
            eprintln!("Failed to initialize clipboard: {}", e);
            return;
        }
    };

    let (_server, mut rx) = match BluetoothServer::start().await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Failed to start Bluetooth server: {}", e);
            return;
        }
    };

    println!("Waiting for phone connection...");

    while let Some(connection) = rx.recv().await {
        println!("Phone connected over Bluetooth!");

        let (mut reader, mut writer) = connection.into_split();
        let read_clipboard = Arc::clone(&clipboard);
        let write_clipboard = Arc::clone(&clipboard);

        let is_internal_write = Arc::new(AtomicBool::new(false));
        let read_flag = Arc::clone(&is_internal_write);
        let write_flag = Arc::clone(&is_internal_write);

        let read_handle = tokio::spawn(async move {
            loop {
                let header_bytes = match reader.read_bytes(5).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("🔌 Phone disconnected (Header Read): {}", e);
                        break;
                    }
                };

                let mut len_bytes = [0u8; 4];
                len_bytes.copy_from_slice(&header_bytes[1..5]);
                let payload_len = u32::from_be_bytes(len_bytes);

                if payload_len > MAX_PAYLOAD_SIZE {
                    eprintln!("Rejected oversized protocol frame: {} bytes", payload_len);
                    break;
                }

                let payload_bytes = match reader.read_bytes(payload_len).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("🔌 Phone disconnected (Payload Read): {}", e);
                        break;
                    }
                };

                let mut raw_frame = Vec::with_capacity(5 + payload_len as usize);
                raw_frame.extend_from_slice(&header_bytes);
                raw_frame.extend_from_slice(&payload_bytes);

                match decode_frame(&raw_frame) {
                    Ok(frame) => match frame.frame_type {
                        FrameType::Text => {
                            if let Ok(text) = String::from_utf8(frame.payload) {
                                println!("Received Text from Phone: '{}'", text);

                                read_flag.store(true, Ordering::SeqCst);
                                let mut clip = read_clipboard.lock().await;
                                let _ = clip.set_text(&text);
                            }
                        }
                        FrameType::Image => {
                            println!("Received Screenshot from Phone ({} bytes)", frame.length);
                            read_flag.store(true, Ordering::SeqCst);
                            let mut clip = read_clipboard.lock().await;
                            if let Err(e) = clip.set_image_from_bytes(&frame.payload) {
                                eprintln!("Failed to write screenshot to OS: {}", e);
                            } else {
                                println!("Screenshot pasted to PC clipboard!");
                            }
                        }
                    },
                    Err(e) => eprintln!("Invalid protocol frame received: {:?}", e),
                }
            }
        });

        let write_handle = tokio::spawn(async move {
            let mut last_copied_text = String::new();

            loop {
                sleep(Duration::from_millis(400)).await;

                let current_text = {
                    let mut clip = write_clipboard.lock().await;
                    clip.get_text().unwrap_or_default()
                };

                if !current_text.is_empty() && current_text != last_copied_text {
                    if write_flag.swap(false, Ordering::SeqCst) {
                        last_copied_text = current_text;
                        continue;
                    }

                    println!("Local PC Text copied: '{}'", current_text);
                    last_copied_text = current_text.clone();

                    let frame = encode_frame(FrameType::Text, current_text.as_bytes());
                    if let Err(e) = writer.write_bytes(&frame).await {
                        eprintln!("Phone disconnected during write: {}", e);
                        break;
                    }
                }
            }
        });

        let _ = tokio::join!(read_handle, write_handle);
        println!("Returned to listening state for next phone connection...");
    }
}
