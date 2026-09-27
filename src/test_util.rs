// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 John Long

//! Helpers shared by the unit tests.

use std::path::{Path, PathBuf};

/// A per-test directory, so tests running in parallel never share files.
/// It's removed on drop, even when an assertion fails.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(test: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("noise-generator-{}-{test}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
