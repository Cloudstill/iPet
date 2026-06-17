//! Test-only utilities. Not compiled outside `cfg(test)`.

use std::fs;
use std::path::{Path, PathBuf};

/// RAII temporary directory. Cleaned up on drop.
///
/// We roll our own instead of pulling in `tempfile` because adding that crate
/// (and its transitive `rustix` chain) caused the test exe to fail to load on
/// Windows with `STATUS_ENTRYPOINT_NOT_FOUND`. The needs here are modest —
/// create a unique dir, drop wipes it.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let mut path = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!("ipet-test-{tag}-{}-{}", std::process::id(), ts));
        fs::create_dir_all(&path).expect("create tempdir");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
