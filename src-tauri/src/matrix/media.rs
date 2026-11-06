//! Buddy Icons: avatar thumbnails, downloaded with authenticated media.
//!
//! The SDK caches downloads in the session's SQLite store, so each icon is
//! fetched from the server once. On top of that, [`Icons`] keeps the ready
//! `data:` URLs in memory so rebuilding the Buddy List is cheap.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use matrix_sdk::{
    Client,
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{OwnedMxcUri, events::room::MediaSource},
};
use tauri::async_runtime::spawn;
use tokio::sync::mpsc::UnboundedSender;

pub struct Icons {
    client: Client,
    /// `None` while downloading, or after a download failed.
    cache: Mutex<HashMap<OwnedMxcUri, Option<String>>>,
    /// Told when a download finishes, so the list can be rebuilt.
    ready: UnboundedSender<()>,
}

impl Icons {
    pub fn new(client: Client, ready: UnboundedSender<()>) -> Arc<Self> {
        Arc::new(Self {
            client,
            cache: Mutex::default(),
            ready,
        })
    }

    /// The icon for `mxc` as a `data:` URL, or `None` if it isn't ready.
    /// The first call starts the download.
    pub fn get(self: &Arc<Self>, mxc: &OwnedMxcUri) -> Option<String> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(icon) = cache.get(mxc) {
            return icon.clone();
        }
        cache.insert(mxc.clone(), None);

        let icons = self.clone();
        let mxc = mxc.clone();
        spawn(async move {
            match icons.fetch(&mxc).await {
                Ok(icon) => {
                    icons.cache.lock().unwrap().insert(mxc, Some(icon));
                    let _ = icons.ready.send(());
                }
                Err(e) => eprintln!("Couldn't fetch buddy icon {mxc}: {e}"),
            }
        });
        None
    }

    async fn fetch(&self, mxc: &OwnedMxcUri) -> matrix_sdk::Result<String> {
        // Shown at 16 px; 32 px stays sharp on high-DPI screens.
        icon(&self.client, mxc, 32).await
    }
}

/// An avatar as a `data:` URL, thumbnailed to `size` px square.
pub async fn icon(client: &Client, mxc: &OwnedMxcUri, size: u32) -> matrix_sdk::Result<String> {
    let request = MediaRequestParameters {
        source: MediaSource::Plain(mxc.clone()),
        format: MediaFormat::Thumbnail(MediaThumbnailSettings::new(size.into(), size.into())),
    };
    let bytes = client.media().get_media_content(&request, true).await?;
    Ok(format!(
        "data:{};base64,{}",
        image_type(&bytes),
        STANDARD.encode(&bytes)
    ))
}

/// The image type, from the file's first bytes.
fn image_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        _ if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_thumbnail_types() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\n...."), "image/png");
        assert_eq!(image_type(b"\xFF\xD8\xFF\xE0...."), "image/jpeg");
        assert_eq!(image_type(b"GIF89a...."), "image/gif");
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(image_type(b"<html>"), "application/octet-stream");
        assert_eq!(image_type(b""), "application/octet-stream");
    }
}
