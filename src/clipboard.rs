use arboard::{Clipboard, ImageData};
use std::borrow::Cow;

pub struct ClipboardManager {
    clipboard: Clipboard,
}

impl ClipboardManager {
    pub fn new() -> Result<Self, String> {
        let clipboard =
            Clipboard::new().map_err(|e| format!("Failed to initialize clipboard: {}", e))?;
        Ok(Self { clipboard })
    }

    pub fn get_text(&mut self) -> Result<String, String> {
        self.clipboard
            .get_text()
            .map_err(|e| format!("Failed to get clipboard text: {}", e))
    }

    pub fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.clipboard
            .set_text(text.to_string())
            .map_err(|e| format!("Failed to set clipboard text: {}", e))
    }

    pub fn set_image_from_bytes(&mut self, compressed_bytes: &[u8]) -> Result<(), String> {
        let dynamic_image = image::load_from_memory(compressed_bytes)
            .map_err(|e| format!("Failed to load image from bytes: {}", e))?;

        let rgba_image = dynamic_image.into_rgb8();
        let (width, height) = rgba_image.dimensions();

        let raw_pixels = rgba_image.into_raw();

        let image_data = ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(raw_pixels),
        };
        self.clipboard
            .set_image(image_data)
            .map_err(|e| format!("failed to set image: {}", e))?;

        Ok(())
    }
}
