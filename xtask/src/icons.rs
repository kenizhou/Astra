// SPDX-License-Identifier: GPL-3.0-only
//! Generates the full icon set from the branding source image.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::imageops::FilterType::Lanczos3;
use image::{DynamicImage, ImageFormat};

const SIZES: [u32; 8] = [1024, 512, 256, 128, 64, 48, 32, 16];
const ICO_SIZES: [u32; 4] = [256, 48, 32, 16];

pub fn generate(src: &Path, out_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let img = image::open(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
    let img = img.into_rgba8();
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
    let mut ico_entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for size in SIZES {
        let resized = image::imageops::resize(&img, size, size, Lanczos3);
        let path = out_dir.join(format!("{size}x{size}.png"));
        image::DynamicImage::ImageRgba8(resized.clone())
            .save_with_format(&path, ImageFormat::Png)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if ICO_SIZES.contains(&size) {
            let mut buf = Vec::new();
            DynamicImage::ImageRgba8(resized)
                .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            ico_entries.push((size, buf));
        }
        written.push(path);
    }
    let ico_path = out_dir.join("icon.ico");
    write_png_ico(&ico_path, &ico_entries)?;
    written.push(ico_path);
    Ok(written)
}

/// Minimal ICO writer embedding PNG frames (Vista and later).
#[allow(clippy::cast_possible_truncation)]
fn write_png_ico(path: &Path, entries: &[(u32, Vec<u8>)]) -> Result<(), String> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0u8, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len();
    for (size, data) in entries {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::checks::test_support::fixture;
    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};

    #[test]
    fn generates_png_set_and_ico() {
        let root = fixture(&[]);
        std::fs::create_dir_all(&root).expect("fixture root");
        let src = root.join("src.jpg");
        let img = ImageBuffer::from_pixel(64, 64, Rgba([10, 20, 30, 255]));
        DynamicImage::ImageRgba8(img)
            .save_with_format(&src, ImageFormat::Jpeg)
            .expect("fixture source");
        let out = root.join("icons");
        let written = super::generate(&src, &out).expect("generate");
        assert_eq!(written.len(), 9);
        assert!(out.join("icon.ico").is_file());
        let (w, h) = image::image_dimensions(out.join("128x128.png")).expect("dims");
        assert_eq!((w, h), (128, 128));
        let head = std::fs::read(out.join("icon.ico")).expect("ico");
        assert_eq!(&head[..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([head[4], head[5]]), 4);
    }
}
