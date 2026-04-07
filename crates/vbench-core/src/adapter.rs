//! The `BenchAdapter` trait that every DB plugin implements.
//!
//! ## Design notes
//!
//! - **Borrowed `VectorRow<'_>`**: zero-copy view over the dataset's vectors.
//!   Adapters that need to own the data (most do — they push it through an
//!   FFI or IPC boundary) take a copy when they need it; we don't force a
//!   `Vec<f32>` allocation on adapters that could ingest by reference.
//!
//! - **`async_trait`**: some adapters (Strata) wrap a synchronous DB in
//!   `tokio::task::spawn_blocking`; others (Qdrant) drive a native gRPC
//!   client. The trait sits at the union, so both fit.
//!
//! - **`optimize` is separate with a no-op default**: HNSW-based adapters
//!   usually want to issue warm-up queries here; sealed-segment adapters
//!   might call a "wait for index" RPC. Adapters that don't need either
//!   inherit the no-op default.
//!
//! - **`search_filtered` is optional**: capability probe via
//!   [`BenchAdapter::supports_filtered_search`]. The runner skips the
//!   filtered phase entirely if the adapter returns `false`.
//!
//! - **Adapter-specific config flows through `serde_json::Value`**: this
//!   keeps `vbench-core` adapter-agnostic. Each adapter documents the schema
//!   it expects on `open()`'s `params` argument.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Distance metric for similarity search.
///
/// vbench-core uses its own simple enum here rather than reaching into any
/// adapter's wire format. Each adapter maps `Metric` to its native
/// representation in `open()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Cosine similarity.
    Cosine,
    /// Euclidean (L2) distance.
    L2,
    /// Inner / dot product.
    Ip,
}

/// A row from a dataset, borrowed for the duration of a `load` call.
///
/// vbench-core hands these to the adapter in batches of [`crate`]-controlled
/// size. The lifetime ties the borrow to the dataset buffer; once `load`
/// returns, the underlying storage may be dropped.
#[derive(Debug, Clone, Copy)]
pub struct VectorRow<'a> {
    /// Stable row id (vbench numbers them 0..N-1).
    pub id: u64,
    /// The embedding.
    pub vector: &'a [f32],
    /// Optional structured labels for filtered-search workloads.
    /// Phase 1 always passes `None`.
    pub labels: Option<&'a serde_json::Value>,
}

/// Self-describing metadata about an adapter, embedded in the result JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    /// Short identifier (e.g. "strata", "qdrant", "instant-distance").
    pub name: String,
    /// The DB version the adapter is targeting (e.g. "0.6.1" for Strata).
    /// Adapters typically populate this from a runtime version probe at
    /// `open()` time, not from a build-time constant.
    pub db_version: String,
    /// Free-form notes — e.g. "embedded, in-process via IPC daemon".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// The trait every DB plugin implements.
///
/// vbench-core's runner is generic over `BenchAdapter`. Each phase (load,
/// optimize, recall, latency) calls into the trait at well-defined points.
#[async_trait]
pub trait BenchAdapter: Send + Sync {
    /// Static metadata about this adapter.
    fn info(&self) -> AdapterInfo;

    /// Open the adapter against a clean working directory.
    ///
    /// `workdir` is provided by the runner (usually a fresh tempdir) and is
    /// owned by this `BenchAdapter` for the duration of the run. The adapter
    /// is responsible for wiping it on `open()` if its DB doesn't already do
    /// so. After `shutdown()`, the runner removes the directory.
    ///
    /// `params` is an opaque adapter-specific config bag — pass anything you
    /// like, document the schema in your adapter's README.
    async fn open(
        workdir: &Path,
        dim: usize,
        metric: Metric,
        params: &serde_json::Value,
    ) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Insert / upsert a batch of rows.
    ///
    /// Implementations should issue a single bulk transaction per batch
    /// where possible — vbench-core sizes batches by the runner's
    /// `--batch-size` flag (default 1000), so per-row overhead is amortised.
    async fn load(&self, rows: &[VectorRow<'_>]) -> anyhow::Result<()>;

    /// Adapter-specific warm-up before the recall phase.
    ///
    /// Default: no-op. Adapters with lazy index construction (Strata,
    /// many HNSW implementations) override this to force the build via
    /// throwaway queries; adapters with explicit "wait for index" RPCs
    /// (Qdrant) override it to call those.
    async fn optimize(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Run a single k-NN query.
    ///
    /// Returns the matched row ids in score order (best first). Latencies
    /// are measured by the runner around this call.
    async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<u64>>;

    /// Whether the adapter supports filtered search (capability probe).
    ///
    /// Default: `false`. The runner skips the filtered phase entirely when
    /// this returns `false`, so adapters without filter support don't need
    /// to implement [`Self::search_filtered`].
    fn supports_filtered_search(&self) -> bool {
        false
    }

    /// Run a filtered k-NN query (only called when
    /// [`Self::supports_filtered_search`] returns `true`).
    async fn search_filtered(
        &self,
        _query: &[f32],
        _k: usize,
        _filter: &serde_json::Value,
    ) -> anyhow::Result<Vec<u64>> {
        anyhow::bail!("filtered search is not supported by this adapter")
    }

    /// Total row count currently loaded.
    ///
    /// Used by the runner to assert all rows landed after the load phase.
    async fn count(&self) -> anyhow::Result<u64>;

    /// Cleanly shut the adapter down.
    ///
    /// Adapters that spawn background processes (e.g. `vbench-strata` spawns
    /// `strata up`) should kill them here. Consumes `self`, so the runner
    /// can't accidentally reuse a half-closed adapter.
    async fn shutdown(self) -> anyhow::Result<()>
    where
        Self: Sized;
}
