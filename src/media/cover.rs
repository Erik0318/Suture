use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context};
use eframe::egui::ColorImage;

use super::tools::sidecar;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif"];

pub fn looks_like_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

pub fn is_image_content(path: &Path) -> bool {
    image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .ok()
        .and_then(|reader| reader.format())
        .is_some()
}

pub fn load_color_image(path: &Path) -> anyhow::Result<ColorImage> {
    let image = image::ImageReader::open(path)
        .with_context(|| format!("Could not open cover {}", path.display()))?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("Unsupported or damaged image {}", path.display()))?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

pub fn discover(folder: &Path, first_track: Option<&Path>) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();

    for stem in ["cover", "folder", "front", "album"] {
        if let Some(path) = entries.iter().find(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(stem))
        }) {
            if load_color_image(path).is_ok() {
                return Some(path.clone());
            }
        }
    }

    if let Some(track) = first_track {
        let target =
            std::env::temp_dir().join(format!("suture-embedded-cover-{}.png", std::process::id()));
        let extracted = Command::new(sidecar("ffmpeg"))
            .args(["-v", "error", "-y", "-i"])
            .arg(track)
            .args(["-map", "0:v:0", "-frames:v", "1"])
            .arg(&target)
            .status()
            .is_ok_and(|status| status.success());
        if extracted && load_color_image(&target).is_ok() {
            return Some(target);
        }
        let _ = fs::remove_file(target);
    }

    entries
        .into_iter()
        .filter(|path| is_image_content(path) && load_color_image(path).is_ok())
        .max_by_key(|path| fs::metadata(path).map(|meta| meta.len()).unwrap_or(0))
}

pub fn validate(path: &Path) -> anyhow::Result<()> {
    load_color_image(path)
        .map(|_| ())
        .map_err(|error| anyhow!(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, io::BufWriter};

    #[test]
    fn detects_image_content_with_unknown_extension() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cover.pic");
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut BufWriter::new(File::create(&path).unwrap()),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert!(is_image_content(&path));
        assert!(load_color_image(&path).is_ok());
    }
}
