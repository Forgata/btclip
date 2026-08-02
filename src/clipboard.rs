use arboard::{Clipboard, ImageData};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;

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

        let rgba_image = dynamic_image.into_rgba8();
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

    /// Checks if the raw image on the clipboard has changed.
    /// Returns Some((new_hash, png_bytes)) if a new image is found.
    pub fn check_image_changed(&mut self, last_hash: u64) -> Option<(u64, Vec<u8>)> {
        let image_data = match self.clipboard.get_image() {
            Ok(data) => data,
            Err(_) => return None,
        };

        let mut hasher = DefaultHasher::new();
        image_data.bytes.hash(&mut hasher);
        let current_hash = hasher.finish();

        if current_hash != last_hash {
            if let Some(rgba_image) = RgbaImage::from_raw(
                image_data.width as u32,
                image_data.height as u32,
                image_data.bytes.into_owned(),
            ) {
                let mut compressed_bytes = Vec::new();
                if DynamicImage::ImageRgba8(rgba_image)
                    .write_to(&mut Cursor::new(&mut compressed_bytes), ImageFormat::Png)
                    .is_ok()
                {
                    return Some((current_hash, compressed_bytes));
                }
            }
        }

        None
    }
}
