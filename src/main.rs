mod bluetooth;
mod clipboard;
mod protocol;

use bluetooth::BluetoothServer;
use clipboard::ClipboardManager;
use protocol::{FrameType, decode_frame, encode_frame};

use std::println;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

const MAX_PAYLOAD_SIZE: u32 = 20 * 1024 * 1024; // 20 MB

#[tokio::main]
async fn main() {
    println!("btclip Windows Host Engine starting...");

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
                        eprintln!("Phone disconnected (Payload Read): {}", e);
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
            let mut last_copied_text = {
                let mut clip = write_clipboard.lock().await;
                clip.get_text().unwrap_or_default()
            };

            let mut last_image_hash: u64 = {
                let mut clip = write_clipboard.lock().await;
                clip.get_file_image_hash()
                    .or_else(|| clip.get_image_hash())
                    .unwrap_or(0)
            };

            loop {
                sleep(Duration::from_millis(400)).await;

                let (text_to_send, image_to_send) = {
                    let mut clip = write_clipboard.lock().await;

                    let text_opt = match clip.get_text() {
                        Ok(text) if !text.is_empty() && text != last_copied_text => Some(text),
                        _ => None,
                    };

                    let file_opt = clip.check_filelist_image(last_image_hash);

                    let img_opt = if file_opt.is_none() {
                        clip.check_image_changed(last_image_hash)
                    } else {
                        None
                    };

                    (text_opt, file_opt.or(img_opt))
                };

                if let Some(text) = text_to_send {
                    if write_flag.swap(false, Ordering::SeqCst) {
                        last_copied_text = text;
                        continue;
                    }

                    println!("Local PC Text copied: '{}'", text);
                    last_copied_text = text.clone();

                    let frame = encode_frame(FrameType::Text, text.as_bytes());
                    if let Err(e) = writer.write_bytes(&frame).await {
                        eprintln!("Phone disconnected during write: {}", e);
                        break;
                    }
                } else if let Some((new_hash, png_bytes)) = image_to_send {
                    if write_flag.swap(false, Ordering::SeqCst) {
                        last_image_hash = new_hash;
                        continue;
                    }

                    let frame = encode_frame(FrameType::Image, &png_bytes);
                    match tokio::time::timeout(Duration::from_secs(20), writer.write_bytes(&frame))
                        .await
                    {
                        Ok(Ok(())) => {
                            last_image_hash = new_hash;
                            println!(
                                "Local PC Image copied! Sent {} bytes successfully.",
                                png_bytes.len()
                            );
                        }
                        Ok(Err(e)) => {
                            eprintln!("Phone disconnected during image write: {}", e);
                            break;
                        }
                        Err(_) => {
                            eprintln!("Phone image write timed out after 20 seconds");
                            break;
                        }
                    }
                }
            }
        });

        let _ = tokio::join!(read_handle, write_handle);
        println!("Returned to listening state for next phone connection...");
    }
}
