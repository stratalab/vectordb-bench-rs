//! vbench command-line interface.
//!
//! Subcommands:
//!
//! ```text
//! vbench list-datasets
//! vbench list-adapters
//! vbench fetch <dataset>
//! vbench run --adapter <name> --dataset <id> [--output <path>] ...
//! vbench inspect <result.json>
//! vbench cache show | clear
//! ```
//!
//! Adapters are gated behind Cargo features. The `strata` feature compiles
//! the Strata adapter into the binary; `all-adapters` enables every adapter
//! known to this version of vbench.

#![warn(missing_docs)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod cmd_cache;
mod cmd_fetch;
mod cmd_inspect;
mod cmd_list;
mod cmd_run;

/// vbench command-line interface.
#[derive(Parser, Debug)]
#[command(
    name = "vbench",
    version,
    about = "Native-Rust vector database benchmark harness",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// List the datasets known to this build.
    ListDatasets,

    /// List the adapters compiled into this build.
    ListAdapters,

    /// Download a dataset to the local cache.
    Fetch {
        /// Dataset id (e.g. "cohere-1m").
        dataset: String,

        /// Override the cache root. Defaults to
        /// `~/.cache/vectordb-bench-rs/datasets/`.
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },

    /// Run a benchmark and produce a TestResult JSON document.
    Run {
        /// Adapter id (e.g. "strata"). Must match a feature compiled in.
        #[arg(long)]
        adapter: String,

        /// Dataset id from the catalog (e.g. "cohere-1m").
        #[arg(long)]
        dataset: String,

        /// Output JSON path. Defaults to
        /// `./vbench-result-<task-label>.json`.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Working directory for the adapter (e.g. the strata daemon's
        /// data dir). Defaults to a fresh tempdir that's removed on exit.
        #[arg(long)]
        workdir: Option<PathBuf>,

        /// Rows per `BenchAdapter::load` call.
        #[arg(long, default_value_t = 1000)]
        batch_size: usize,

        /// k for recall@k and ndcg@k. Defaults to upstream's K_DEFAULT.
        #[arg(long, default_value_t = 100)]
        recall_k: usize,

        /// Warm-up queries to issue during the optimize phase.
        #[arg(long, default_value_t = 200)]
        warmup_queries: usize,

        /// Free-form label for the published result.
        #[arg(long)]
        task_label: Option<String>,

        /// Adapter-specific: explicit path to the `strata` binary.
        /// Only meaningful when `--adapter strata`.
        #[arg(long)]
        strata_bin: Option<PathBuf>,

        /// Override the dataset cache root.
        #[arg(long)]
        cache_dir: Option<PathBuf>,

        /// Don't remove the workdir after the run completes (useful for
        /// post-mortem debugging of the adapter's data files).
        #[arg(long)]
        keep_workdir: bool,
    },

    /// Pretty-print a TestResult JSON document.
    Inspect {
        /// Path to a result JSON file.
        path: PathBuf,
    },

    /// Show or clear the dataset cache.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
}

/// Subcommands of `vbench cache`.
#[derive(Subcommand, Debug)]
enum CacheAction {
    /// Show the cache root and the datasets currently downloaded.
    Show {
        /// Override the cache root.
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
    /// Remove all datasets from the cache. Asks for confirmation.
    Clear {
        /// Override the cache root.
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Commands::ListDatasets => cmd_list::list_datasets(),
        Commands::ListAdapters => cmd_list::list_adapters(),
        Commands::Fetch { dataset, cache_dir } => cmd_fetch::fetch(&dataset, cache_dir).await,
        Commands::Run {
            adapter,
            dataset,
            output,
            workdir,
            batch_size,
            recall_k,
            warmup_queries,
            task_label,
            strata_bin,
            cache_dir,
            keep_workdir,
        } => {
            cmd_run::run(cmd_run::RunArgs {
                adapter,
                dataset,
                output,
                workdir,
                batch_size,
                recall_k,
                warmup_queries,
                task_label,
                strata_bin,
                cache_dir,
                keep_workdir,
            })
            .await
        }
        Commands::Inspect { path } => cmd_inspect::inspect(&path),
        Commands::Cache { action } => match action {
            CacheAction::Show { cache_dir } => cmd_cache::show(cache_dir),
            CacheAction::Clear { cache_dir, yes } => cmd_cache::clear(cache_dir, yes),
        },
    }
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("vbench=info,info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
