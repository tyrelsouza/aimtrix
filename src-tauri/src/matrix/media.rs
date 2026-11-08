//! Buddy Icons: avatar thumbnails, downloaded with authenticated media.
//!
//! The SDK caches downloads in the session's SQLite store, so each icon is
//! fetched from the server once. On top of that, [`Icons`] keeps the ready
//! `data:` URLs in memory so rebuilding the Buddy List is cheap.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use matrix_sdk::{
    Client,
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{OwnedMxcUri, events::room::MediaSource},
};
use tauri::async_runtime::spawn;
use tokio::sync::mpsc::UnboundedSender;

/// How long after a failed download to try again: long enough not to hammer
/// a server that's down, short enough that icons come back after a blip
/// (or after starting offline).
const RETRY_AFTER: Duration = Duration::from_secs(60);

enum Entry {
    Downloading,
    Ready(String),
    Failed(Instant),
}

pub struct Icons {
    client: Client,
    cache: Mutex<HashMap<OwnedMxcUri, Entry>>,
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
    /// The first call starts the download; a failed one is tried again
    /// after [`RETRY_AFTER`], by a later call.
    pub fn get(self: &Arc<Self>, mxc: &OwnedMxcUri) -> Option<String> {
        let mut cache = self.cache.lock().unwrap();
        match cache.get(mxc) {
            Some(Entry::Ready(icon)) => return Some(icon.clone()),
            Some(Entry::Downloading) => return None,
            Some(Entry::Failed(at)) if at.elapsed() < RETRY_AFTER => return None,
            Some(Entry::Failed(_)) | None => {}
        }
        cache.insert(mxc.clone(), Entry::Downloading);

        let icons = self.clone();
        let mxc = mxc.clone();
        spawn(async move {
            let entry = match icons.fetch(&mxc).await {
                Ok(icon) => Entry::Ready(icon),
                Err(e) => {
                    eprintln!("Couldn't fetch buddy icon {mxc}: {e}");
                    Entry::Failed(Instant::now())
                }
            };
            let ready = matches!(entry, Entry::Ready(_));
            icons.cache.lock().unwrap().insert(mxc, entry);
            if ready {
                let _ = icons.ready.send(());
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
    Ok(data_url(&bytes))
}

/// An image file as a `data:` URL a window can show.
pub fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:{};base64,{}",
        image_type(bytes),
        STANDARD.encode(bytes)
    )
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
