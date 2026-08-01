mod bluetooth;
mod clipboard;
mod protocol;

use bluetooth::{BluetoothConnection, BluetoothServer};
use std::{assert_eq, io::Cursor, println};

use clipboard::ClipboardManager;
use image::{DynamicImage, ImageFormat, RgbImage};

#[tokio::main]
async fn main() {
    let mut clipboard_manager = match ClipboardManager::new() {
        Ok(manager) => manager,
        Err(e) => {
            println!("Failed to initialize clipboard manager: {}", e);
            return;
        }
    };

    println!("testing text write");
    clipboard_manager.set_text("hello frim firgata!").unwrap();

    let current_text = clipboard_manager.get_text().unwrap();
    println!("Current clipboard text: {}", current_text);
    assert_eq!(current_text, "hello frim firgata!");

    println!("testing image write");
    let mut image = RgbImage::new(100, 100);

    for pixel in image.pixels_mut() {
        *pixel = image::Rgb([0, 0, 255]);
    }

    let mut compressed_bytes: Vec<u8> = Vec::new();

    DynamicImage::ImageRgb8(image)
        .write_to(&mut Cursor::new(&mut compressed_bytes), ImageFormat::Png)
        .expect("Failed to write image to buffer");

    clipboard_manager
        .set_image_from_bytes(&compressed_bytes)
        .expect("Failed to set image from bytes");

    println!("Image written to clipboard successfully.");

    println!("Starting Bluetooth RFCOMM server...");

    let (_server, mut rx): (BluetoothServer, tokio::sync::mpsc::Receiver<_>) =
        match BluetoothServer::start().await {
            Ok(server) => server,
            Err(e) => {
                println!("Failed to start Bluetooth server: {}", e);
                return;
            }
        };

    if let Some(mut connection) = rx.recv().await {
        println!("Connection accepted from Bluetooth client!");

        let test_payload = b"Hello from Rust PC!";
        let frame = protocol::encode_frame(protocol::FrameType::Text, test_payload);

        if let Err(e) = connection.write_bytes(&frame).await {
            eprintln!("Failed to send welcome payload: {}", e);
        } else {
            println!("Sent test welcome frame over Bluetooth socket!");
        }
    }
}
