//! Sending files and images: reading the file, working out its type, and
//! for images their size and a thumbnail, as Element does. The upload
//! itself (encrypted in encrypted rooms) is [`super::timeline::Watching`]'s.

use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use image::{GenericImageView, ImageFormat, imageops::FilterType};
use matrix_sdk::{
    attachment::{AttachmentInfo, BaseFileInfo, BaseImageInfo, Thumbnail},
    ruma::UInt,
};
use matrix_sdk_ui::timeline::AttachmentConfig;
use mime::Mime;

/// Thumbnails fit in this box, like Element's. Smaller images aren't given
/// one; clients show the image itself.
const THUMBNAIL_SIZE: (u32, u32) = (800, 600);

/// Images bigger than this aren't decoded for a thumbnail: decoding takes
/// a lot of memory, and they still send fine without one.
const MAX_THUMBNAIL_SOURCE: u64 = 30 * 1024 * 1024;

/// A file ready to send.
pub struct Prepared {
    pub name: String,
    pub bytes: Vec<u8>,
    pub mime: Mime,
    pub config: AttachmentConfig,
}

/// A file size for people: "512 bytes", "34 KB", "1.5 MB", "2.1 GB".
pub fn describe_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} bytes")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB)
    } else {
        format!("{:.1} GB", b / KB / KB / KB)
    }
}

/// The name a file is sent under.
pub fn file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Can't send {}: it has no usable name.", path.display()))
}

/// Something to send: a file, or bytes pasted from the clipboard.
pub enum Source {
    File(PathBuf),
    Pasted { name: String, bytes: Vec<u8> },
}

impl Source {
    pub fn name(&self) -> Result<String, String> {
        match self {
            Self::File(path) => file_name(path),
            Self::Pasted { name, .. } => Ok(name.clone()),
        }
    }

    /// Its size in bytes. Errors for folders and files that can't be read.
    pub fn size(&self) -> Result<u64, String> {
        match self {
            Self::File(path) => {
                let name = file_name(path)?;
                let meta =
                    std::fs::metadata(path).map_err(|e| format!("Couldn't read {name}: {e}"))?;
                if !meta.is_file() {
                    return Err(format!("{name} is a folder; send the files in it instead."));
                }
                Ok(meta.len())
            }
            Self::Pasted { bytes, .. } => Ok(bytes.len() as u64),
        }
    }
}

/// The picture on the system clipboard, as a PNG, or `None` if there isn't
/// one. For pastes the webview doesn't hand over (WebKitGTK keeps pasted
/// pictures to itself). Blocking.
pub fn clipboard_picture() -> Result<Option<Vec<u8>>, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| format!("Couldn't open the clipboard: {e}"))?;
    let picture = match clipboard.get_image() {
        Ok(picture) => picture,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(e) => return Err(format!("Couldn't read the picture on the clipboard: {e}")),
    };
    let (width, height) = (picture.width as u32, picture.height as u32);
    let pixels = image::RgbaImage::from_raw(width, height, picture.bytes.into_owned())
        .ok_or("The picture on the clipboard is damaged.")?;
    let mut png = Vec::new();
    image::DynamicImage::from(pixels)
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|e| format!("Couldn't read the picture on the clipboard: {e}"))?;
    Ok(Some(png))
}

/// The name for a pasted picture of type `mime`, e.g. "Pasted image.png".
pub fn pasted_name(mime: &str) -> String {
    let ext = mime_guess::get_mime_extensions_str(mime)
        .and_then(|exts| {
            // The list isn't in order of preference; pick the usual ones.
            ["png", "jpg", "gif", "webp"]
                .into_iter()
                .find(|e| exts.contains(e))
                .or(exts.first().copied())
        })
        .unwrap_or("bin");
    format!("Pasted image.{ext}")
}

/// Read `source` and describe it. Blocking: reads the file and may decode
/// an image.
pub fn prepare(source: Source) -> Result<Prepared, String> {
    let name = source.name()?;
    let bytes = match source {
        Source::File(path) => {
            std::fs::read(&path).map_err(|e| format!("Couldn't read {name}: {e}"))?
        }
        Source::Pasted { bytes, .. } => bytes,
    };
    let mime = mime_guess::from_path(&name).first_or_octet_stream();
    let size = UInt::try_from(bytes.len() as u64).ok();
    let config = match image_info(&bytes, &mime) {
        Some((info, thumbnail)) => AttachmentConfig {
            info: Some(AttachmentInfo::Image(BaseImageInfo { size, ..info })),
            thumbnail,
            ..Default::default()
        },
        None => AttachmentConfig {
            info: Some(AttachmentInfo::File(BaseFileInfo { size })),
            ..Default::default()
        },
    };
    Ok(Prepared {
        name,
        bytes,
        // An image we can't read goes as a plain file, so clients don't try
        // to show it.
        mime: match &config.info {
            Some(AttachmentInfo::File(_)) if mime.type_() == mime::IMAGE => {
                mime::APPLICATION_OCTET_STREAM
            }
            _ => mime,
        },
        config,
    })
}

/// An image's size, and a thumbnail if it's big. `None` if it isn't an
/// image we can read.
fn image_info(bytes: &[u8], mime: &Mime) -> Option<(BaseImageInfo, Option<Thumbnail>)> {
    if mime.type_() != mime::IMAGE {
        return None;
    }
    let format = ImageFormat::from_mime_type(mime.essence_str())?;
    let reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let (width, height) = reader.into_dimensions().ok()?;
    let info = BaseImageInfo {
        width: Some(width.into()),
        height: Some(height.into()),
        is_animated: (format == ImageFormat::Gif).then_some(true),
        ..Default::default()
    };
    let too_big_to_decode = bytes.len() as u64 > MAX_THUMBNAIL_SOURCE;
    let small = width <= THUMBNAIL_SIZE.0 && height <= THUMBNAIL_SIZE.1;
    // An animated GIF's first frame would show as a still.
    if small || too_big_to_decode || format == ImageFormat::Gif {
        return Some((info, None));
    }
    Some((info, thumbnail(bytes, format)))
}

/// A JPEG (or PNG, to keep transparency) that fits in [`THUMBNAIL_SIZE`].
fn thumbnail(bytes: &[u8], format: ImageFormat) -> Option<Thumbnail> {
    let image = image::load_from_memory_with_format(bytes, format).ok()?;
    let small = image.resize(THUMBNAIL_SIZE.0, THUMBNAIL_SIZE.1, FilterType::Triangle);
    let (width, height) = small.dimensions();
    let (out_format, content_type, small) = if small.color().has_alpha() {
        (ImageFormat::Png, mime::IMAGE_PNG, small)
    } else {
        (
            ImageFormat::Jpeg,
            mime::IMAGE_JPEG,
            small.into_rgb8().into(),
        )
    };
    let mut data = Vec::new();
    small
        .write_to(&mut Cursor::new(&mut data), out_format)
        .ok()?;
    Some(Thumbnail {
        size: UInt::try_from(data.len() as u64).ok()?,
        data,
        content_type,
        width: width.into(),
        height: height.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, height, image::Rgb([200, 30, 30]));
        let mut bytes = Vec::new();
        image::DynamicImage::from(image)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn pasted_pictures_get_a_name_with_the_right_extension() {
        assert_eq!(pasted_name("image/png"), "Pasted image.png");
        assert_eq!(pasted_name("image/jpeg"), "Pasted image.jpg");
        assert_eq!(pasted_name("nonsense"), "Pasted image.bin");
        let prepared = prepare(Source::Pasted {
            name: pasted_name("image/png"),
            bytes: png(10, 10),
        })
        .unwrap();
        assert_eq!(prepared.mime, mime::IMAGE_PNG);
        assert!(matches!(
            prepared.config.info,
            Some(AttachmentInfo::Image(_))
        ));
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(describe_size(512), "512 bytes");
        assert_eq!(describe_size(34 * 1024), "34 KB");
        assert_eq!(describe_size(1536 * 1024), "1.5 MB");
        assert_eq!(describe_size(50 * 1024 * 1024), "50.0 MB");
    }

    #[test]
    fn big_images_get_their_size_and_a_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let prepared =
            prepare(Source::File(write(dir.path(), "big.png", &png(1600, 900)))).unwrap();
        assert_eq!(prepared.name, "big.png");
        assert_eq!(prepared.mime, mime::IMAGE_PNG);
        let Some(AttachmentInfo::Image(info)) = &prepared.config.info else {
            panic!("not sent as an image");
        };
        assert_eq!(
            (info.width, info.height),
            (Some(1600u32.into()), Some(900u32.into()))
        );
        assert_eq!(info.size, Some((prepared.bytes.len() as u32).into()));
        let thumbnail = prepared.config.thumbnail.expect("no thumbnail");
        assert_eq!(
            (thumbnail.width, thumbnail.height),
            (800u32.into(), 450u32.into())
        );
        assert_eq!(thumbnail.content_type, mime::IMAGE_JPEG);
    }

    #[test]
    fn small_images_go_without_a_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let prepared = prepare(Source::File(write(dir.path(), "small.png", &png(40, 30)))).unwrap();
        assert!(matches!(
            prepared.config.info,
            Some(AttachmentInfo::Image(_))
        ));
        assert!(prepared.config.thumbnail.is_none());
    }

    #[test]
    fn other_files_and_broken_images_go_as_files() {
        let dir = tempfile::tempdir().unwrap();
        let notes = prepare(Source::File(write(dir.path(), "notes.txt", b"hello"))).unwrap();
        assert_eq!(notes.mime, mime::TEXT_PLAIN);
        assert!(matches!(
            notes.config.info,
            Some(AttachmentInfo::File(BaseFileInfo { size: Some(s) })) if s == 5u32.into()
        ));
        let broken = prepare(Source::File(write(dir.path(), "broken.png", b"not a png"))).unwrap();
        assert_eq!(broken.mime, mime::APPLICATION_OCTET_STREAM);
        assert!(matches!(broken.config.info, Some(AttachmentInfo::File(_))));
    }
}
