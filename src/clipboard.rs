use arboard::{Clipboard, ImageData};
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::ffi::OsString;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr;
use winapi::um::shellapi::DragQueryFileW;
use winapi::um::winbase::{GlobalLock, GlobalSize, GlobalUnlock};
use winapi::um::winuser::{
    CF_DIB, CF_DIBV5, CF_HDROP, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable,
    OpenClipboard,
};

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
        let image_result = self.clipboard.get_image();

        let (current_hash, compressed_bytes) = match image_result {
            Ok(image_data) => {
                let mut hasher = DefaultHasher::new();
                image_data.bytes.hash(&mut hasher);
                let current_hash = hasher.finish();

                let png_bytes = if let Some(rgba_image) = RgbaImage::from_raw(
                    image_data.width as u32,
                    image_data.height as u32,
                    image_data.bytes.into_owned(),
                ) {
                    let mut compressed_bytes = Vec::new();
                    if DynamicImage::ImageRgba8(rgba_image)
                        .write_to(&mut Cursor::new(&mut compressed_bytes), ImageFormat::Png)
                        .is_ok()
                    {
                        Some(compressed_bytes)
                    } else {
                        None
                    }
                } else {
                    None
                };

                (current_hash, png_bytes)
            }
            Err(_) => {
                if let Some((hash, png_bytes)) = self.load_clipboard_dib_image() {
                    return if hash != last_hash {
                        Some((hash, png_bytes))
                    } else {
                        None
                    };
                }
                return None;
            }
        };

        if let Some(png_bytes) = compressed_bytes {
            if current_hash != last_hash {
                return Some((current_hash, png_bytes));
            }
        }

        None
    }

    /// Return a fast hash of the raw clipboard image if present, without
    /// performing PNG compression. Useful to seed `last_image_hash` at startup
    /// to avoid an initial send of the current clipboard image.
    pub fn get_image_hash(&mut self) -> Option<u64> {
        match self.clipboard.get_image() {
            Ok(image_data) => {
                let mut hasher = DefaultHasher::new();
                image_data.bytes.hash(&mut hasher);
                Some(hasher.finish())
            }
            Err(_) => self.load_clipboard_dib_hash(),
        }
    }

    fn load_clipboard_dib_hash(&mut self) -> Option<u64> {
        self.load_clipboard_dib_image().map(|(hash, _)| hash)
    }

    fn load_clipboard_dib_image(&mut self) -> Option<(u64, Vec<u8>)> {
        unsafe {
            if OpenClipboard(ptr::null_mut()) == 0 {
                return None;
            }

            let format = if IsClipboardFormatAvailable(CF_DIBV5) != 0 {
                CF_DIBV5
            } else if IsClipboardFormatAvailable(CF_DIB) != 0 {
                CF_DIB
            } else {
                CloseClipboard();
                return None;
            };

            let handle = GetClipboardData(format);
            if handle.is_null() {
                CloseClipboard();
                return None;
            }

            let mem = GlobalLock(handle as _);
            if mem.is_null() {
                CloseClipboard();
                return None;
            }

            let size = GlobalSize(handle as _) as usize;
            if size == 0 {
                GlobalUnlock(handle as _);
                CloseClipboard();
                return None;
            }

            let slice = std::slice::from_raw_parts(mem as *const u8, size);
            let header_size = u32::from_le_bytes(slice.get(0..4)?.try_into().ok()?) as usize;
            let palette_bytes = if header_size >= 40 && slice.len() >= 40 {
                let bit_count = u16::from_le_bytes(slice.get(14..16)?.try_into().ok()?);
                let colors_used = u32::from_le_bytes(slice.get(32..36)?.try_into().ok()?);
                let num_colors = if colors_used != 0 {
                    colors_used
                } else if bit_count <= 8 {
                    1 << bit_count
                } else {
                    0
                };
                (num_colors as usize).saturating_mul(4)
            } else {
                0
            };

            let bmp_offset = 14 + header_size + palette_bytes;
            let mut bmp = Vec::with_capacity(14 + size);
            bmp.extend_from_slice(b"BM");
            bmp.extend_from_slice(&((14 + size) as u32).to_le_bytes());
            bmp.extend_from_slice(&0u16.to_le_bytes());
            bmp.extend_from_slice(&0u16.to_le_bytes());
            bmp.extend_from_slice(&(bmp_offset as u32).to_le_bytes());
            bmp.extend_from_slice(slice);

            GlobalUnlock(handle as _);
            CloseClipboard();

            let image = image::load_from_memory(&bmp).ok()?;
            let rgba_image = image.into_rgba8();
            let mut hasher = DefaultHasher::new();
            rgba_image.as_raw().hash(&mut hasher);
            let current_hash = hasher.finish();

            let mut compressed_bytes = Vec::new();
            if DynamicImage::ImageRgba8(rgba_image)
                .write_to(&mut Cursor::new(&mut compressed_bytes), ImageFormat::Png)
                .is_ok()
            {
                return Some((current_hash, compressed_bytes));
            }

            None
        }
    }

    pub fn get_file_image_hash(&mut self) -> Option<u64> {
        self.check_filelist_image(0).map(|(hash, _)| hash)
    }

    /// Try to read an image file path from the clipboard CF_HDROP and load it.
    /// Returns Some((hash, png_bytes)) on success.
    pub fn check_filelist_image(&mut self, last_hash: u64) -> Option<(u64, Vec<u8>)> {
        unsafe {
            if OpenClipboard(ptr::null_mut()) != 0 {
                if IsClipboardFormatAvailable(CF_HDROP) != 0 {
                    let h = GetClipboardData(CF_HDROP);
                    if !h.is_null() {
                        let count = DragQueryFileW(h as _, 0xFFFFFFFFu32, ptr::null_mut(), 0);
                        if count > 0 {
                            let len = DragQueryFileW(h as _, 0, ptr::null_mut(), 0) as usize;
                            if len > 0 {
                                let mut buf: Vec<u16> = vec![0u16; len + 1];
                                let copied =
                                    DragQueryFileW(h as _, 0, buf.as_mut_ptr(), (len + 1) as u32);
                                if copied > 0 {
                                    let mut trimmed = &buf[..copied as usize];
                                    while let Some(&0) = trimmed.last() {
                                        trimmed = &trimmed[..trimmed.len() - 1];
                                    }
                                    let path = OsString::from_wide(trimmed);
                                    let p = PathBuf::from(path);
                                    if p.exists() {
                                        if let Ok(bytes) = fs::read(&p) {
                                            if let Ok(img) = image::load_from_memory(&bytes) {
                                                let rgba_image = img.into_rgba8();
                                                let mut hasher = DefaultHasher::new();
                                                rgba_image.as_raw().hash(&mut hasher);
                                                let current_hash = hasher.finish();
                                                if current_hash != last_hash {
                                                    let mut compressed_bytes = Vec::new();
                                                    if DynamicImage::ImageRgba8(rgba_image)
                                                        .write_to(
                                                            &mut Cursor::new(&mut compressed_bytes),
                                                            ImageFormat::Png,
                                                        )
                                                        .is_ok()
                                                    {
                                                        CloseClipboard();
                                                        return Some((
                                                            current_hash,
                                                            compressed_bytes,
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                CloseClipboard();
            }
        }

        None
    }
}
