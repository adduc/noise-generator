//! Persists EQ and volume between runs in an INI-style file under the XDG
//! config dir (usually ~/.config/noise-generator/settings.ini). GLib's
//! KeyFile is used instead of GSettings, which would need an installed schema.

use std::path::{Path, PathBuf};

use gtk::glib::{self, KeyFile, KeyFileFlags};

use crate::noise::{self, BAND_COUNT, MAX_DB, MIN_DB};

const EQ_GROUP: &str = "eq";
const OUTPUT_GROUP: &str = "output";
const VOLUME_KEY: &str = "volume";
const DEFAULT_VOLUME: f32 = 0.5;

pub struct Settings {
    pub band_db: [f32; BAND_COUNT],
    pub volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { band_db: [0.0; BAND_COUNT], volume: DEFAULT_VOLUME }
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
        let band_db = std::array::from_fn(|band| {
            file.double(EQ_GROUP, &band_key(band))
                .map_or(0.0, |db| (db as f32).clamp(MIN_DB, MAX_DB))
        });
        let volume = file
            .double(OUTPUT_GROUP, VOLUME_KEY)
            .map_or(DEFAULT_VOLUME, |v| (v as f32).clamp(0.0, 1.0));
        Self { band_db, volume }
    }

    fn save_to(&self, path: &Path) -> Result<(), glib::Error> {
        let file = KeyFile::new();
        for (band, db) in self.band_db.iter().enumerate() {
            file.set_double(EQ_GROUP, &band_key(band), *db as f64);
        }
        file.set_double(OUTPUT_GROUP, VOLUME_KEY, self.volume as f64);

        if let Some(dir) = path.parent() {
            // Surface a directory error through the save error below.
            let _ = std::fs::create_dir_all(dir);
        }
        file.save_to_file(path)
    }
}

fn path() -> PathBuf {
    glib::user_config_dir().join("noise-generator").join("settings.ini")
}

/// Keys like `band_31`, `band_1k` keep the file readable and hand-editable.
fn band_key(band: usize) -> String {
    format!("band_{}", noise::band_label(band))
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
        let saved = Settings { band_db: noise::Preset::Pink.band_db(), volume: 0.3 };
        saved.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path);
        assert_eq!(loaded.band_db, saved.band_db);
        assert_eq!(loaded.volume, saved.volume);
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
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let loaded = Settings::load_from(&temp_dir("missing").join("settings.ini"));
        assert_eq!(loaded.band_db, [0.0; BAND_COUNT]);
        assert_eq!(loaded.volume, DEFAULT_VOLUME);
    }
}
