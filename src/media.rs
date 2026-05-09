use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaKind {
    Static,
    Live,
}

pub fn classify_path(path: &Path) -> Result<MediaKind> {
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str())
        && let Some(kind) = classify_extension(ext)
    {
        return Ok(kind);
    }

    classify_signature(path)
        .ok_or_else(|| anyhow::anyhow!("unsupported wallpaper media type: {}", path.display()))
}

pub fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(classify_extension)
        .is_some()
}

fn classify_extension(ext: &str) -> Option<MediaKind> {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif" => Some(MediaKind::Static),
        "mp4" | "webm" | "mkv" | "mov" | "avi" => Some(MediaKind::Live),
        _ => None,
    }
}

fn classify_signature(path: &Path) -> Option<MediaKind> {
    let bytes = fs::read(path).ok()?;
    if bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"BM")
        || is_riff_type(&bytes, b"WEBP")
    {
        return Some(MediaKind::Static);
    }

    if is_isobmff_video(&bytes) || bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return Some(MediaKind::Live);
    }

    None
}

fn is_riff_type(bytes: &[u8], kind: &[u8; 4]) -> bool {
    bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == kind
}

fn is_isobmff_video(bytes: &[u8]) -> bool {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }

    let major = &bytes[8..12];
    matches!(
        major,
        b"avif" | b"iso2" | b"isom" | b"mp41" | b"mp42" | b"qt  "
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_static_extensions() {
        assert_eq!(
            classify_path(Path::new("wall.png")).unwrap(),
            MediaKind::Static
        );
        assert_eq!(
            classify_path(Path::new("wall.WEBP")).unwrap(),
            MediaKind::Static
        );
    }

    #[test]
    fn classifies_live_extensions() {
        assert_eq!(
            classify_path(Path::new("rain.mp4")).unwrap(),
            MediaKind::Live
        );
        assert_eq!(
            classify_path(Path::new("rain.MKV")).unwrap(),
            MediaKind::Live
        );
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert!(classify_path(Path::new("notes.txt")).is_err());
    }

    #[test]
    fn falls_back_to_file_signature_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wall");
        fs::write(&path, b"\x89PNG\r\n\x1a\nrest").unwrap();

        assert_eq!(classify_path(&path).unwrap(), MediaKind::Static);
    }
}
