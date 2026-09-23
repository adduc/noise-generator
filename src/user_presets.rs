//! User-saved EQ presets, stored as one INI group per preset in
//! presets.ini next to settings.ini:
//!
//! ```ini
//! [Rainy night]
//! band_31=6
//! ...
//! ```

use std::path::PathBuf;

use gtk::glib::{self, KeyFile, KeyFileFlags};

use crate::noise::BAND_COUNT;
use crate::settings;

pub struct PresetLibrary {
    path: PathBuf,
    file: KeyFile,
}

impl PresetLibrary {
    pub fn open() -> Self {
        Self::open_at(settings::config_dir().join("presets.ini"))
    }

    fn open_at(path: PathBuf) -> Self {
        let file = KeyFile::new();
        if let Err(err) = file.load_from_file(&path, KeyFileFlags::KEEP_COMMENTS) {
            // A file that exists but can't be parsed would be overwritten by
            // the next save, losing every preset. Move it aside instead.
            if !err.matches(glib::FileError::Noent) && path.exists() {
                let backup = path.with_extension("ini.bak");
                eprintln!("unreadable presets file ({err}); moving it to {}", backup.display());
                let _ = std::fs::rename(&path, &backup);
            }
        }
        Self { path, file }
    }

    /// Preset names, sorted case-insensitively.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> =
            self.file.groups().iter().map(|g| g.to_string()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    pub fn get(&self, name: &str) -> Option<[f32; BAND_COUNT]> {
        self.file
            .has_group(name)
            .then(|| settings::read_bands(&self.file, name))
    }

    /// Saves (or overwrites) a preset. `name` must pass [`is_valid_name`].
    pub fn save(&self, name: &str, band_db: &[f32; BAND_COUNT]) -> Result<(), glib::Error> {
        settings::write_bands(&self.file, name, band_db);
        settings::save_key_file(&self.file, &self.path)
    }

    pub fn delete(&self, name: &str) -> Result<(), glib::Error> {
        self.file.remove_group(name)?;
        settings::save_key_file(&self.file, &self.path)
    }
}

/// INI group names can't contain brackets or control characters, and
/// surrounding whitespace would be confusing, so callers should trim first.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && !name.chars().any(|c| c == '[' || c == ']' || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(test: &str) -> PathBuf {
        std::env::temp_dir().join(format!("noise-generator-{}-{test}", std::process::id()))
    }

    #[test]
    fn save_get_delete_round_trip_through_disk() {
        let dir = temp_dir("presets_round_trip");
        let path = dir.join("presets.ini");
        let bands: [f32; BAND_COUNT] = std::array::from_fn(|i| i as f32 - 4.0);

        let library = PresetLibrary::open_at(path.clone());
        library.save("rainy Night", &bands).unwrap();
        library.save("Airplane cabin ✈", &[3.0; BAND_COUNT]).unwrap();
        library.save("Brownish", &[-1.0; BAND_COUNT]).unwrap();

        // Reopen to prove everything went through the file.
        let library = PresetLibrary::open_at(path.clone());
        assert_eq!(library.names(), ["Airplane cabin ✈", "Brownish", "rainy Night"]);
        assert_eq!(library.get("rainy Night"), Some(bands));
        assert_eq!(library.get("missing"), None);

        library.save("Brownish", &[-2.0; BAND_COUNT]).unwrap(); // overwrite
        library.delete("Airplane cabin ✈").unwrap();

        let library = PresetLibrary::open_at(path);
        assert_eq!(library.names(), ["Brownish", "rainy Night"]);
        assert_eq!(library.get("Brownish"), Some([-2.0; BAND_COUNT]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_backed_up_not_overwritten() {
        let dir = temp_dir("presets_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("presets.ini");
        std::fs::write(&path, "this is not an ini file\n").unwrap();

        let library = PresetLibrary::open_at(path.clone());
        assert!(library.names().is_empty());
        library.save("New", &[0.0; BAND_COUNT]).unwrap();

        let backup = std::fs::read_to_string(dir.join("presets.ini.bak")).unwrap();
        assert_eq!(backup, "this is not an ini file\n");
        assert_eq!(PresetLibrary::open_at(path).names(), ["New"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validates_names() {
        assert!(is_valid_name("Rainy night"));
        assert!(is_valid_name("Café ☕ 2"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name(" padded "));
        assert!(!is_valid_name("a[b]"));
        assert!(!is_valid_name("line\nbreak"));
    }
}
