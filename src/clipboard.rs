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
use winapi::um::winuser::{
    CF_HDROP, CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
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
        let image_data = match self.clipboard.get_image() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("clipboard: get_image() failed: {:?}", e);
                return None;
            }
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

    /// Return a fast hash of the raw clipboard image if present, without
    /// performing PNG compression. Useful to seed `last_image_hash` at startup
    /// to avoid an initial send of the current clipboard image.
    pub fn get_image_hash(&mut self) -> Option<u64> {
        let image_data = match self.clipboard.get_image() {
            Ok(data) => data,
            Err(_) => return None,
        };

        let mut hasher = DefaultHasher::new();
        image_data.bytes.hash(&mut hasher);
        Some(hasher.finish())
    }

    /// Try to read an image file path from the clipboard CF_HDROP and load it.
    /// Returns Some((hash, png_bytes)) on success.
    pub fn check_filelist_image(&mut self, last_hash: u64) -> Option<(u64, Vec<u8>)> {
        unsafe {
            if OpenClipboard(ptr::null_mut()) != 0 {
                if IsClipboardFormatAvailable(CF_HDROP) != 0 {
                    let h = GetClipboardData(CF_HDROP);
                    if !h.is_null() {
                        eprintln!("clipboard: CF_HDROP handle received");
                        let count = DragQueryFileW(h as _, 0xFFFFFFFFu32, ptr::null_mut(), 0);
                        eprintln!("clipboard: CF_HDROP file count = {}", count);
                        if count > 0 {
                            let len = DragQueryFileW(h as _, 0, ptr::null_mut(), 0) as usize;
                            eprintln!("clipboard: CF_HDROP first file path length = {}", len);
                            if len > 0 {
                                let mut buf: Vec<u16> = vec![0u16; len + 1];
                                let copied =
                                    DragQueryFileW(h as _, 0, buf.as_mut_ptr(), (len + 1) as u32);
                                eprintln!("clipboard: CF_HDROP copied chars = {}", copied);
                                if copied > 0 {
                                    let mut trimmed = &buf[..copied as usize];
                                    while let Some(&0) = trimmed.last() {
                                        trimmed = &trimmed[..trimmed.len() - 1];
                                    }
                                    eprintln!("clipboard: CF_HDROP raw utf16 = {:?}", trimmed);
                                    let path = OsString::from_wide(trimmed);
                                    let path_str = path.to_string_lossy();
                                    eprintln!("clipboard: CF_HDROP path = {}", path_str);
                                    let p = PathBuf::from(path);
                                    if p.exists() {
                                        eprintln!("clipboard: CF_HDROP path exists");
                                        match fs::read(&p) {
                                            Ok(bytes) => {
                                                eprintln!(
                                                    "clipboard: CF_HDROP read {} bytes",
                                                    bytes.len()
                                                );
                                                match image::load_from_memory(&bytes) {
                                                    Ok(img) => {
                                                        let rgba_image = img.into_rgba8();
                                                        let mut hasher = DefaultHasher::new();
                                                        rgba_image.as_raw().hash(&mut hasher);
                                                        let current_hash = hasher.finish();
                                                        eprintln!(
                                                            "clipboard: CF_HDROP image loaded {}x{} hash={} last_hash={}",
                                                            rgba_image.width(),
                                                            rgba_image.height(),
                                                            current_hash,
                                                            last_hash
                                                        );
                                                        if current_hash != last_hash {
                                                            let mut compressed_bytes = Vec::new();
                                                            match DynamicImage::ImageRgba8(
                                                                rgba_image,
                                                            )
                                                            .write_to(
                                                                &mut Cursor::new(
                                                                    &mut compressed_bytes,
                                                                ),
                                                                ImageFormat::Png,
                                                            ) {
                                                                Ok(_) => {
                                                                    eprintln!(
                                                                        "clipboard: CF_HDROP PNG compression succeeded"
                                                                    );
                                                                    CloseClipboard();
                                                                    return Some((
                                                                        current_hash,
                                                                        compressed_bytes,
                                                                    ));
                                                                }
                                                                Err(e) => {
                                                                    eprintln!(
                                                                        "clipboard: CF_HDROP PNG compression failed: {:?}",
                                                                        e
                                                                    );
                                                                }
                                                            }
                                                        } else {
                                                            eprintln!(
                                                                "clipboard: CF_HDROP image hash matches last_hash"
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        eprintln!(
                                                            "clipboard: CF_HDROP image load failed: {:?}",
                                                            e
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "clipboard: CF_HDROP failed to read file: {:?}",
                                                    e
                                                );
                                            }
                                        }
                                    } else {
                                        eprintln!("clipboard: CF_HDROP path does not exist");
                                    }
                                }
                            }
                        }
                    } else {
                        eprintln!("clipboard: CF_HDROP data handle is null");
                    }
                } else {
                    eprintln!("clipboard: CF_HDROP format not available");
                }
                CloseClipboard();
            } else {
                eprintln!("clipboard: failed to open clipboard");
            }
        }

        None
    }
}
