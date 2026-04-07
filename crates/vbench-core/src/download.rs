//! Async HTTP downloader for the VectorDBBench-hosted dataset bundles.
//!
//! Streams files from the spec's `url_base` into the dataset's cache
//! directory. Implements three durability properties:
//!
//! 1. **Atomic per-file writes** — each file is downloaded to `<name>.tmp`
//!    then `rename`d into place once the body finishes streaming. A
//!    process killed mid-download leaves a `.tmp` file that the next run
//!    discards.
//!
//! 2. **Bundle-level marker** — only after all required files are in place
//!    do we touch [`crate::cache::MARKER_FILE`]. The marker is the only
//!    signal the loader trusts; partial bundles re-download.
//!
//! 3. **Idempotent** — if the marker file already exists,
//!    [`ensure_dataset_downloaded`] returns immediately without making any
//!    HTTP requests.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::cache::{cache_dir_for, default_cache_root, is_cache_complete, write_marker};
use crate::dataset::DatasetSpec;
use crate::error::{Result, VbenchError};

/// Ensure the given dataset is fully downloaded into `cache_root`.
///
/// On return, every required file (`train`, `test`, `neighbors`) and the
/// `.complete` marker exist in the dataset's cache subdirectory.
///
/// `cache_root` is typically the result of [`default_cache_root`]; tests
/// pass a `tempfile::TempDir` path.
///
/// `progress` is invoked once per file with `(file_name, bytes_so_far,
/// total_bytes_or_zero_if_unknown)`. Pass a no-op closure if you don't
/// care about progress.
pub async fn ensure_dataset_downloaded(
    spec: &DatasetSpec,
    cache_root: Option<&Path>,
    mut progress: impl FnMut(&str, u64, u64) + Send + 'static,
) -> Result<PathBuf> {
    let owned_root;
    let cache_root = match cache_root {
        Some(p) => p.to_path_buf(),
        None => {
            owned_root = default_cache_root();
            owned_root.clone()
        }
    };
    let dir = cache_dir_for(&cache_root, spec.cache_subdir);
    std::fs::create_dir_all(&dir)?;

    if is_cache_complete(&dir) {
        return Ok(dir);
    }

    let client = reqwest::Client::builder()
        .user_agent("vectordb-bench-rs/0.1")
        .build()
        .map_err(|e| VbenchError::InvalidInput(format!("reqwest build: {e}")))?;

    for file_name in [spec.train_file, spec.test_file, spec.neighbors_file] {
        let url = format!("{}{}", spec.url_base, file_name);
        let final_path = dir.join(file_name);
        let tmp_path = dir.join(format!("{file_name}.tmp"));

        // If the final file already exists from a previous interrupted run
        // (the .complete marker was missing), drop it and re-download to
        // avoid trusting a corrupt half-file.
        if final_path.exists() {
            std::fs::remove_file(&final_path)?;
        }
        if tmp_path.exists() {
            std::fs::remove_file(&tmp_path)?;
        }

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| VbenchError::InvalidInput(format!("GET {url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(VbenchError::InvalidInput(format!(
                "GET {url}: HTTP {}",
                resp.status()
            )));
        }

        let total = resp.content_length().unwrap_or(0);
        let mut so_far: u64 = 0;
        let mut tmp = tokio::fs::File::create(&tmp_path).await?;
        let mut body = resp.bytes_stream();

        while let Some(chunk) = body.next().await {
            let chunk =
                chunk.map_err(|e| VbenchError::InvalidInput(format!("read body {url}: {e}")))?;
            tmp.write_all(&chunk).await?;
            so_far += chunk.len() as u64;
            progress(file_name, so_far, total);
        }
        tmp.flush().await?;
        tmp.sync_all().await?;
        drop(tmp);

        std::fs::rename(&tmp_path, &final_path)?;
    }

    write_marker(&dir)?;
    Ok(dir)
}
