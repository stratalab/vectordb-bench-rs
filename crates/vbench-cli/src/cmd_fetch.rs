//! `vbench fetch <dataset>` — download a dataset to the local cache.

use std::path::PathBuf;

use vbench_core::{ensure_dataset_downloaded, get_spec};

pub async fn fetch(dataset_id: &str, cache_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let spec =
        get_spec(dataset_id).ok_or_else(|| anyhow::anyhow!("unknown dataset id: {dataset_id}"))?;

    let approx_gb = spec.approx_download_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    println!(
        "Fetching {} (approx {:.1} GiB)…",
        spec.display_name, approx_gb
    );

    let path = ensure_dataset_downloaded(spec, cache_dir.as_deref(), |file, so_far, total| {
        if total > 0 {
            let pct = (so_far as f64 / total as f64) * 100.0;
            eprint!("\r  {file}: {pct:6.1}%");
        } else {
            eprint!("\r  {file}: {} MiB", so_far / (1024 * 1024));
        }
    })
    .await?;

    eprintln!();
    println!("Done. Cache: {}", path.display());
    Ok(())
}
