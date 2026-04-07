//! On-disk cache for downloaded datasets.
//!
//! Layout:
//!
//! ```text
//! $HOME/.cache/vectordb-bench-rs/datasets/<spec.cache_subdir>/
//!     train.parquet
//!     test.parquet
//!     neighbors.parquet
//!     .complete         # marker file written after every required file landed
//! ```
//!
//! The marker file is the only signal that the cache directory is usable.
//! Without it, [`crate::download::ensure_dataset_downloaded`] re-downloads.
//! Partial downloads (process killed mid-stream) leave a directory without
//! the marker, so the next run heals itself.

use std::path::{Path, PathBuf};

/// Returns the default cache root for vbench, e.g.
/// `~/.cache/vectordb-bench-rs/datasets/` on Linux.
///
/// Falls back to `./.cache/vectordb-bench-rs/datasets/` if `dirs::cache_dir`
/// returns `None` (very rare; only on environments without HOME).
pub fn default_cache_root() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("./.cache"));
    base.join("vectordb-bench-rs").join("datasets")
}

/// Returns the cache directory for a specific dataset, given the cache root.
///
/// Joins `cache_root / cache_subdir` and returns the resulting path. Does
/// not create the directory; that's the downloader's job.
pub fn cache_dir_for(cache_root: &Path, cache_subdir: &str) -> PathBuf {
    cache_root.join(cache_subdir)
}

/// Marker file name written after every required file is in place.
pub(crate) const MARKER_FILE: &str = ".complete";

/// Whether the cache directory is "complete" (has the marker file).
pub fn is_cache_complete(dir: &Path) -> bool {
    dir.join(MARKER_FILE).exists()
}

/// Touch the marker file. Caller must ensure all required files are present.
pub(crate) fn write_marker(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join(MARKER_FILE), b"vbench-cache-complete\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn marker_round_trip() {
        let dir = TempDir::new().unwrap();
        assert!(!is_cache_complete(dir.path()));
        write_marker(dir.path()).unwrap();
        assert!(is_cache_complete(dir.path()));
    }

    #[test]
    fn cache_dir_for_joins_subdir() {
        let root = PathBuf::from("/tmp/vbench");
        let dir = cache_dir_for(&root, "cohere_medium_1m");
        assert_eq!(dir, PathBuf::from("/tmp/vbench/cohere_medium_1m"));
    }

    #[test]
    fn default_cache_root_ends_in_datasets() {
        let root = default_cache_root();
        assert!(root.ends_with("vectordb-bench-rs/datasets"));
    }
}
