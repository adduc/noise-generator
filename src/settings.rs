// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 John Long

//! Persists EQ, the active preset, volume and window size between runs in an
//! INI-style file under the XDG config dir (usually
//! ~/.config/noise-generator/settings.ini). GLib's KeyFile is used instead of
//! GSettings, which would need an installed schema.

use std::path::{Path, PathBuf};

use gtk::glib::{self, KeyFile, KeyFileFlags};

use crate::noise::{self, BAND_COUNT, MAX_DB, MIN_DB, Preset};
use crate::player::PresetRef;

const EQ_GROUP: &str = "eq";
// Separate keys, so a saved preset named like a built-in one stays distinct.
const BUILTIN_PRESET_KEY: &str = "builtin_preset";
const USER_PRESET_KEY: &str = "user_preset";
const OUTPUT_GROUP: &str = "output";
const VOLUME_KEY: &str = "volume";
const DEFAULT_VOLUME: f32 = 0.5;
const WINDOW_GROUP: &str = "window";
const WIDTH_KEY: &str = "width";
const HEIGHT_KEY: &str = "height";
const MAXIMIZED_KEY: &str = "maximized";

pub struct Settings {
    pub band_db: [f32; BAND_COUNT],
    /// The preset `band_db` came from, if the EQ wasn't hand-tweaked.
    pub preset: Option<PresetRef>,
    pub volume: f32,
    pub window: WindowState,
}

/// The window's unmaximized size, and whether it was maximized. A `None`
/// size means the window opens at its natural size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowState {
    pub size: Option<(i32, i32)>,
    pub maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            band_db: [0.0; BAND_COUNT],
            preset: None,
            volume: DEFAULT_VOLUME,
            window: WindowState::default(),
        }
    }
}

impl Settings {
    /// Loads saved settings. Missing or malformed values fall back to their
    /// defaults individually, so a hand-edited file never prevents startup.
    pub fn load() -> Self {
        Self::load_from(&path())
    }

    pub fn save(&self) -> Result<(), glib::Error> {
        self.save_to(&path())
    }

    fn load_from(path: &Path) -> Self {
        let file = KeyFile::new();
        if file.load_from_file(path, KeyFileFlags::NONE).is_err() {
            return Self::default();
        }
        let band_db = read_bands(&file, EQ_GROUP);
        let preset = read_preset(&file);
        let volume = file
            .double(OUTPUT_GROUP, VOLUME_KEY)
            .map_or(DEFAULT_VOLUME, |v| (v as f32).clamp(0.0, 1.0));
        Self {
            band_db,
            preset,
            volume,
            window: read_window(&file),
        }
    }

    fn save_to(&self, path: &Path) -> Result<(), glib::Error> {
        let file = KeyFile::new();
        write_bands(&file, EQ_GROUP, &self.band_db);
        match &self.preset {
            Some(PresetRef::Builtin(preset)) => {
                file.set_string(EQ_GROUP, BUILTIN_PRESET_KEY, preset.label())
            }
            Some(PresetRef::User(name)) => file.set_string(EQ_GROUP, USER_PRESET_KEY, name),
            None => {}
        }
        file.set_double(OUTPUT_GROUP, VOLUME_KEY, self.volume as f64);
        if let Some((width, height)) = self.window.size {
            file.set_integer(WINDOW_GROUP, WIDTH_KEY, width);
            file.set_integer(WINDOW_GROUP, HEIGHT_KEY, height);
        }
        file.set_boolean(WINDOW_GROUP, MAXIMIZED_KEY, self.window.maximized);
        save_key_file(&file, path)
    }
}

/// Reads the active preset. An unknown built-in label counts as no preset;
/// whether a user preset still exists is up to the caller.
fn read_preset(file: &KeyFile) -> Option<PresetRef> {
    if let Ok(label) = file.string(EQ_GROUP, BUILTIN_PRESET_KEY) {
        return Preset::from_label(&label).map(PresetRef::Builtin);
    }
    let name = file.string(EQ_GROUP, USER_PRESET_KEY).ok()?;
    Some(PresetRef::User(name.to_string()))
}

/// Reads the window state. The size is used only if both dimensions are
/// present and positive; GTK itself enforces the minimum size.
fn read_window(file: &KeyFile) -> WindowState {
    let width = file
        .integer(WINDOW_GROUP, WIDTH_KEY)
        .ok()
        .filter(|w| *w > 0);
    let height = file
        .integer(WINDOW_GROUP, HEIGHT_KEY)
        .ok()
        .filter(|h| *h > 0);
    WindowState {
        size: width.zip(height),
        maximized: file.boolean(WINDOW_GROUP, MAXIMIZED_KEY).unwrap_or(false),
    }
}

pub fn config_dir() -> PathBuf {
    glib::user_config_dir().join("noise-generator")
}

fn path() -> PathBuf {
    config_dir().join("settings.ini")
}

/// Keys like `band_31`, `band_1k` keep the file readable and hand-editable.
fn band_key(band: usize) -> String {
    format!("band_{}", noise::band_label(band))
}

/// Reads EQ levels from `group`. Each missing or malformed band falls back
/// to 0 dB on its own, and out-of-range values are clamped.
pub fn read_bands(file: &KeyFile, group: &str) -> [f32; BAND_COUNT] {
    std::array::from_fn(|band| {
        file.double(group, &band_key(band))
            .map_or(0.0, |db| (db as f32).clamp(MIN_DB, MAX_DB))
    })
}

pub fn write_bands(file: &KeyFile, group: &str, band_db: &[f32; BAND_COUNT]) {
    for (band, db) in band_db.iter().enumerate() {
        file.set_double(group, &band_key(band), *db as f64);
    }
}

/// Writes `file` to `path`, creating parent directories as needed.
pub fn save_key_file(file: &KeyFile, path: &Path) -> Result<(), glib::Error> {
    if let Some(dir) = path.parent() {
        // Surface a directory error through the save error below.
        let _ = std::fs::create_dir_all(dir);
    }
    file.save_to_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-test directory, so tests running in parallel never share files.
    fn temp_dir(test: &str) -> PathBuf {
        std::env::temp_dir().join(format!("noise-generator-{}-{test}", std::process::id()))
    }

    #[test]
    fn round_trips_through_file() {
        let dir = temp_dir("round_trip");
        let path = dir.join("nested/settings.ini");
        let saved = Settings {
            band_db: Preset::Pink.band_db(),
            preset: Some(PresetRef::Builtin(Preset::Pink)),
            volume: 0.3,
            window: WindowState {
                size: Some((640, 480)),
                maximized: true,
            },
        };
        saved.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.band_db, saved.band_db);
        assert_eq!(loaded.preset, saved.preset);
        assert_eq!(loaded.volume, saved.volume);
        assert_eq!(loaded.window, saved.window);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trips_user_preset() {
        let dir = temp_dir("user_preset");
        let path = dir.join("settings.ini");
        // A user preset may share a built-in's label; it must stay a user preset.
        let saved = Settings {
            preset: Some(PresetRef::User("Pink".into())),
            ..Default::default()
        };
        saved.save_to(&path).unwrap();

        assert_eq!(Settings::load_from(&path).preset, saved.preset);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_builtin_preset_is_ignored() {
        let dir = temp_dir("unknown_builtin");
        let path = dir.join("settings.ini");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "[eq]\nbuiltin_preset=Plaid\n").unwrap();

        assert_eq!(Settings::load_from(&path).preset, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_values_fall_back_individually() {
        let dir = temp_dir("bad_values");
        let path = dir.join("settings.ini");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "[eq]\nband_1k=99\nband_31=oops\nband_125=-6\n").unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.band_db[5], MAX_DB); // clamped
        assert_eq!(loaded.band_db[0], 0.0); // unparsable -> default
        assert_eq!(loaded.band_db[2], -6.0);
        assert_eq!(loaded.volume, DEFAULT_VOLUME); // missing -> default
        assert_eq!(loaded.window, WindowState::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let loaded = Settings::load_from(&temp_dir("missing").join("settings.ini"));
        assert_eq!(loaded.band_db, [0.0; BAND_COUNT]);
        assert_eq!(loaded.preset, None);
        assert_eq!(loaded.volume, DEFAULT_VOLUME);
    }

    fn key_file(contents: &str) -> KeyFile {
        let file = KeyFile::new();
        file.load_from_data(contents, KeyFileFlags::NONE).unwrap();
        file
    }

    #[test]
    fn partial_or_invalid_size_is_ignored() {
        let only_width = read_window(&key_file("[window]\nwidth=800\n"));
        assert_eq!(only_width.size, None);
        let negative = read_window(&key_file(
            "[window]\nwidth=-5\nheight=600\nmaximized=true\n",
        ));
        assert_eq!(
            negative,
            WindowState {
                size: None,
                maximized: true
            }
        );
    }
}
