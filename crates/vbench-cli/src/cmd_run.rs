//! `vbench run` — the main benchmark entry point.
//!
//! Pulls the dataset (downloading and decoding if necessary), opens the
//! requested adapter, drives it through `vbench-core`'s runner, and writes
//! the resulting `TestResult` JSON to the configured output path.

use std::path::PathBuf;

use tracing::info;
#[cfg(feature = "strata")]
use vbench_core::BenchAdapter;
use vbench_core::{
    ensure_dataset_downloaded, get_spec, parquet_io, run_benchmark, LoadedDataset, RunnerOptions,
};

#[allow(missing_docs)]
pub struct RunArgs {
    pub adapter: String,
    pub dataset: String,
    pub output: Option<PathBuf>,
    pub workdir: Option<PathBuf>,
    pub batch_size: usize,
    pub recall_k: usize,
    pub warmup_queries: usize,
    pub task_label: Option<String>,
    pub strata_bin: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
    pub keep_workdir: bool,
}

pub async fn run(args: RunArgs) -> anyhow::Result<()> {
    let spec = get_spec(&args.dataset)
        .ok_or_else(|| anyhow::anyhow!("unknown dataset id: {}", args.dataset))?;

    // -------- 1. ensure dataset is on disk --------
    info!(dataset = spec.id, "ensuring dataset cache");
    let cache_dir =
        ensure_dataset_downloaded(spec, args.cache_dir.as_deref(), |file, so_far, total| {
            if total > 0 {
                let pct = (so_far as f64 / total as f64) * 100.0;
                eprint!("\r  fetching {file}: {pct:6.1}%");
            }
        })
        .await?;
    eprintln!();

    // -------- 2. decode parquet → in-memory dataset --------
    info!(
        train = ?cache_dir.join(spec.train_file),
        test  = ?cache_dir.join(spec.test_file),
        gt    = ?cache_dir.join(spec.neighbors_file),
        "decoding parquet files"
    );
    let (n_train, train_flat) =
        parquet_io::read_embeddings_parquet(&cache_dir.join(spec.train_file), spec.dim)?;
    if n_train != spec.num_train {
        anyhow::bail!(
            "{}: train.parquet has {} rows but spec says {}",
            spec.id,
            n_train,
            spec.num_train
        );
    }
    let (n_test, test_flat) =
        parquet_io::read_embeddings_parquet(&cache_dir.join(spec.test_file), spec.dim)?;
    if n_test != spec.num_test {
        anyhow::bail!(
            "{}: test.parquet has {} rows but spec says {}",
            spec.id,
            n_test,
            spec.num_test
        );
    }
    let ground_truth = parquet_io::read_neighbours_parquet(&cache_dir.join(spec.neighbors_file))?;

    let dataset = LoadedDataset::from_buffers(spec, train_flat, test_flat, ground_truth)?;
    info!(
        memory_mib = dataset.memory_bytes() / (1024 * 1024),
        "dataset loaded into memory"
    );

    // -------- 3. resolve workdir --------
    let workdir_owner: WorkdirOwner = if let Some(p) = args.workdir.clone() {
        std::fs::create_dir_all(&p)?;
        WorkdirOwner::User(p)
    } else {
        WorkdirOwner::Tempdir(tempfile::TempDir::new()?)
    };
    let workdir = workdir_owner.path().to_path_buf();
    info!(workdir = ?workdir, "adapter workdir");

    // -------- 4. build runner options --------
    let task_label = args
        .task_label
        .clone()
        .unwrap_or_else(|| format!("{}-{}", args.adapter, args.dataset));
    let opts = RunnerOptions {
        batch_size: args.batch_size,
        recall_k: args.recall_k,
        warmup_queries: args.warmup_queries,
        task_label: task_label.clone(),
        db_note: Some("installed via stratadb.org/install.sh".to_string()),
    };

    // -------- 5. dispatch on adapter id --------
    // Explicit type annotation: when the `strata` feature is off, both
    // surviving match arms call `anyhow::bail!` (return `!`), so the
    // compiler can't otherwise infer `result`'s type.
    let result: vbench_core::TestResult = match args.adapter.as_str() {
        #[cfg(feature = "strata")]
        "strata" => {
            let mut params = serde_json::json!({});
            if let Some(p) = args.strata_bin {
                params["strata_bin"] = serde_json::json!(p);
            }
            let adapter =
                vbench_strata::StrataAdapter::open(&workdir, spec.dim, spec.metric, &params)
                    .await?;
            run_benchmark(adapter, &dataset, &opts).await?
        }
        #[cfg(not(feature = "strata"))]
        "strata" => {
            anyhow::bail!(
                "this vbench was built without the `strata` feature. Rebuild with \
                 `cargo install vectordb-bench-rs --features strata`"
            );
        }
        other => anyhow::bail!("unknown adapter: {other}"),
    };

    // -------- 6. write result JSON --------
    let out_path = args
        .output
        .unwrap_or_else(|| PathBuf::from(format!("vbench-result-{task_label}.json")));
    let json = result.to_json()?;
    std::fs::write(&out_path, &json)?;
    println!();
    println!("Result written to {}", out_path.display());
    println!(
        "  recall@{}      : {:.4}",
        opts.recall_k, result.results[0].metrics.recall
    );
    println!(
        "  ndcg@{}        : {:.4}",
        opts.recall_k, result.results[0].metrics.ndcg
    );
    println!(
        "  load_duration : {:.2} s ({:.2} insert + {:.2} optimize)",
        result.results[0].metrics.load_duration,
        result.results[0].metrics.insert_duration,
        result.results[0].metrics.optimize_duration,
    );
    println!(
        "  serial p99    : {:.2} ms",
        result.results[0].metrics.serial_latency_p99 * 1000.0
    );

    // -------- 7. workdir cleanup --------
    if args.keep_workdir {
        if let WorkdirOwner::Tempdir(td) = workdir_owner {
            // Persist the tempdir so the user can poke at it.
            let kept = td.keep();
            println!("Workdir kept at: {}", kept.display());
        }
    }
    // Otherwise: WorkdirOwner::Tempdir drops here and removes the dir;
    // WorkdirOwner::User leaves it alone.

    Ok(())
}

/// Owns the adapter's workdir for the duration of the run. Either a
/// caller-supplied path (which we don't touch on exit) or a fresh tempdir
/// (which is removed on drop unless `--keep-workdir` is set).
enum WorkdirOwner {
    User(PathBuf),
    Tempdir(tempfile::TempDir),
}

impl WorkdirOwner {
    fn path(&self) -> &std::path::Path {
        match self {
            WorkdirOwner::User(p) => p,
            WorkdirOwner::Tempdir(td) => td.path(),
        }
    }
}
