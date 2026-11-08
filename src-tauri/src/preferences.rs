//! Preferences: auto-reply, the tray and fonts. Kept as JSON in the app's config
//! folder (they aren't secret), shared by every account on this computer.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// Event carrying the new [`Preferences`] to every window.
pub const EVENT: &str = "preferences:update";

/// Font faces offered for reading conversations, as CSS font names. The
/// empty name is the app's own font. They only change how messages look
/// here; Matrix has no portable way to send a font.
pub const FONT_FACES: &[&str] = &[
    "",
    "Arial",
    "Comic Sans MS",
    "Courier New",
    "Georgia",
    "Times New Roman",
    "Trebuchet MS",
    "Verdana",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontSize {
    Small,
    #[default]
    Normal,
    Large,
    Larger,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    /// While away, answer each conversation's first message with the away
    /// message.
    pub auto_reply: bool,
    /// The color your messages are sent in, as `#rrggbb`.
    pub text_color: String,
    /// One of [`FONT_FACES`].
    pub font_face: String,
    pub font_size: FontSize,
    /// Closing the Buddy List hides it in the tray and AIMtrix keeps
    /// running, when there's a tray (`tray.rs`). Off by default on Linux,
    /// where trays are often missing; on by default on Windows.
    pub close_to_tray: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            auto_reply: false,
            text_color: "#000000".to_owned(),
            font_face: String::new(),
            font_size: FontSize::Normal,
            close_to_tray: cfg!(target_os = "windows"),
        }
    }
}

impl Preferences {
    /// These, with anything out of range put back to its default.
    fn checked(mut self) -> Self {
        let hex = self.text_color.len() == 7
            && self.text_color.starts_with('#')
            && self.text_color[1..].chars().all(|c| c.is_ascii_hexdigit());
        if hex {
            self.text_color.make_ascii_lowercase();
        } else {
            self.text_color = Self::default().text_color;
        }
        if !FONT_FACES.contains(&self.font_face.as_str()) {
            self.font_face = String::new();
        }
        self
    }
}

/// The saved preferences, or the defaults if there are none (or the file
/// is damaged). A value that can't be read (from a newer version, or edited
/// by hand) falls back to its default on its own; the others are kept.
fn load(path: &Path) -> Preferences {
    let Ok(bytes) = std::fs::read(path) else {
        return Preferences::default();
    };
    let saved = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(saved)) => saved,
        _ => {
            eprintln!("Ignoring damaged preferences in {}", path.display());
            return Preferences::default();
        }
    };
    let mut merged = serde_json::to_value(Preferences::default()).expect("preferences serialize");
    for (key, value) in saved {
        let mut tried = merged.clone();
        tried[&key] = value;
        if serde_json::from_value::<Preferences>(tried.clone()).is_ok() {
            merged = tried;
        } else {
            eprintln!("Ignoring the preference {key} in {}", path.display());
        }
    }
    serde_json::from_value::<Preferences>(merged)
        .map(Preferences::checked)
        .unwrap_or_default()
}

/// Save, replacing the file in one step so a crash can't leave half of it.
fn save(path: &Path, prefs: &Preferences) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't save preferences: {e}"))?;
    }
    let json = serde_json::to_vec_pretty(prefs).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("Couldn't save preferences: {e}"))
}

/// The app's preferences and where they're saved.
pub struct Store {
    path: PathBuf,
    current: Mutex<Preferences>,
}

impl Store {
    /// Load the preferences saved for this app, if any.
    pub fn open(app: &AppHandle) -> tauri::Result<Self> {
        let path = app.path().app_config_dir()?.join("preferences.json");
        let current = Mutex::new(load(&path));
        Ok(Self { path, current })
    }

    pub fn get(&self) -> Preferences {
        self.current.lock().unwrap().clone()
    }

    /// Check, save and share new preferences. Returns them as saved.
    pub fn set(&self, app: &AppHandle, prefs: Preferences) -> Result<Preferences, String> {
        let prefs = prefs.checked();
        save(&self.path, &prefs)?;
        *self.current.lock().unwrap() = prefs.clone();
        if let Err(e) = app.emit(EVENT, &prefs) {
            eprintln!("Couldn't send preferences to windows: {e}");
        }
        Ok(prefs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_damaged_files_give_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        assert_eq!(load(&path), Preferences::default());
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Preferences::default());
        assert!(
            !Preferences::default().auto_reply,
            "auto-reply is off by default"
        );
    }

    #[test]
    fn preferences_survive_a_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        // In a folder that doesn't exist yet, like a first run.
        let path = dir.path().join("config").join("preferences.json");
        let prefs = Preferences {
            auto_reply: true,
            text_color: "#0000ff".to_owned(),
            font_face: "Georgia".to_owned(),
            font_size: FontSize::Large,
            close_to_tray: true,
        };
        save(&path, &prefs).unwrap();
        assert_eq!(load(&path), prefs);
    }

    #[test]
    fn out_of_range_values_go_back_to_defaults() {
        let checked = Preferences {
            text_color: "red; background: url(x)".to_owned(),
            font_face: "Wingdings\"; }".to_owned(),
            ..Default::default()
        }
        .checked();
        assert_eq!(checked.text_color, "#000000");
        assert_eq!(checked.font_face, "");
        let upper = Preferences {
            text_color: "#00FF00".to_owned(),
            ..Default::default()
        }
        .checked();
        assert_eq!(upper.text_color, "#00ff00");
    }

    #[test]
    fn one_bad_value_leaves_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(
            &path,
            r#"{ "autoReply": true, "fontSize": "huge", "fontFace": "Georgia" }"#,
        )
        .unwrap();
        let prefs = load(&path);
        assert!(prefs.auto_reply);
        assert_eq!(prefs.font_face, "Georgia");
        assert_eq!(prefs.font_size, FontSize::Normal);
    }

    #[test]
    fn older_files_missing_newer_settings_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        std::fs::write(&path, r#"{ "autoReply": true }"#).unwrap();
        let prefs = load(&path);
        assert!(prefs.auto_reply);
        assert_eq!(prefs.font_size, FontSize::Normal);
    }
}
