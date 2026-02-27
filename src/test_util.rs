#![cfg(test)]

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// RAII temp directory — cleaned up on drop (even on panic).
pub struct TempSandbox(PathBuf);

impl TempSandbox {
    pub fn new() -> Self {
        let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("tod_test_{}_{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// Create the sandbox with a `src/main.rs` already present.
    pub fn with_main_rs() -> Self {
        let sb = Self::new();
        fs::create_dir_all(sb.join("src")).unwrap();
        fs::write(sb.join("src/main.rs"), "fn main() {}\n").unwrap();
        sb
    }
}

impl Deref for TempSandbox {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
