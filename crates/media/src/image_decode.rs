use std::path::Path;

use crate::error::MediaError;
use crate::video::Frame;

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            IMAGE_EXTENSIONS.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

pub(crate) fn image_dimensions(path: &Path) -> Result<(u32, u32), MediaError> {
    image::image_dimensions(path).map_err(|error| MediaError::Unreadable {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

/// Decodes a still image (the first frame of an animated one) to BGRA.
pub fn decode_image(path: &Path) -> Result<Frame, MediaError> {
    let image = image::open(path).map_err(|error| MediaError::Unreadable {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut bgra = rgba.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(Frame {
        width,
        height,
        time_seconds: 0.0,
        bgra,
    })
}
