//! `TestResult` schema — wire-compatible with VectorDBBench.
//!
//! Field names mirror VectorDBBench's `vectordb_bench/models.py:TestResult`
//! so reviewers can drop our JSON into the existing leaderboard tooling
//! without a translation step.
//!
//! ## Critical units
//!
//! VectorDBBench is inconsistent about latency/duration units, and getting
//! this wrong is the most common way for a benchmark result to be quietly
//! incomparable. The conventions:
//!
//! | Field                       | Unit         |
//! |-----------------------------|--------------|
//! | `load_duration`             | seconds (f64) |
//! | `optimize_duration`         | seconds (f64) |
//! | `serial_latency_p99`        | milliseconds (f64) |
//! | `serial_latency_p95`        | milliseconds (f64) |
//! | `serial_latency_p50`        | milliseconds (f64) |
//! | `serial_latency_avg`        | milliseconds (f64) |
//! | `conc_latency_p99_list`     | milliseconds (Vec<f64>) |
//! | `qps`                       | queries / second (f64) |
//!
//! These are documented inline on each field. The `tests/result_schema.rs`
//! integration test serialises a synthetic result and asserts every key name
//! matches the upstream schema, so renaming a field on either side breaks
//! the build instead of silently producing incomparable numbers.

use serde::{Deserialize, Serialize};

use crate::host::HostInfo;

/// Top-level test result document.
///
/// One per benchmark run. Serialised to JSON via `serde_json::to_string_pretty`
/// and dropped into `results/<date>/<adapter>-<dataset>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// VectorDBBench schema version this document targets.
    /// Hardcoded to "1" for now; bump if upstream's schema breaks.
    pub vbench_schema_version: String,

    /// Free-form human label for this run (e.g. "strata-0.6.1-cohere-1m").
    /// Surfaces in the leaderboard table verbatim.
    pub task_label: String,

    /// Adapter / DB configuration captured at run start.
    pub db_config: DbConfig,

    /// Workload configuration (dataset, recall_k, batch size, …).
    pub case_config: CaseConfig,

    /// Phase-by-phase task configuration (which phases ran, with what knobs).
    pub task_config: TaskConfig,

    /// The numbers.
    pub metrics: ResultMetrics,

    /// Provenance: when did this run happen, on what host.
    pub timestamps: Timestamps,
}

/// Adapter / DB configuration.
///
/// Captured at run start so the leaderboard reflects exactly what was tested,
/// even if the adapter or DB version changes between runs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DbConfig {
    /// Adapter id (e.g. "strata", "qdrant").
    pub adapter: String,
    /// DB version captured at run-time, not a build-time constant.
    /// For Strata this is set to the value returned by IPC `Ping`.
    pub db_version: String,
    /// How the DB binary was sourced (e.g. "stratadb.org/install.sh",
    /// "PATH", "/usr/local/bin/strata").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_method: Option<String>,
    /// HNSW M parameter, if known. Strata 0.6.x doesn't expose this via
    /// the executor API, so for the Strata adapter this is documented as
    /// "default-not-tunable-in-0.6.0" rather than left blank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hnsw_m: Option<String>,
    /// HNSW efConstruction, same caveat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hnsw_ef_construction: Option<String>,
    /// HNSW efSearch, same caveat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hnsw_ef_search: Option<String>,
    /// Free-form notes — e.g. "embedded, in-process via IPC daemon".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Workload configuration: which dataset, what recall_k, what batch size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseConfig {
    /// Dataset id from the catalog (e.g. "cohere-1m").
    pub dataset: String,
    /// Vector dimensionality.
    pub dim: usize,
    /// Distance metric used.
    pub metric: String,
    /// k for recall@k computation.
    pub recall_k: usize,
    /// Number of training vectors loaded.
    pub num_train: usize,
    /// Number of test queries used.
    pub num_test: usize,
}

/// Phase configuration: what knobs the runner applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Batch size for the load phase (rows per `BenchAdapter::load` call).
    pub batch_size: usize,
    /// Warm-up queries issued during the optimize phase.
    pub warmup_queries: usize,
    /// Whether the concurrent QPS phase was run (Phase 2 only).
    pub run_concurrent: bool,
}

/// The actual numbers.
///
/// **Units matter** — see the module-level table.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResultMetrics {
    /// Wall-clock load duration in **seconds**.
    pub load_duration: f64,
    /// Wall-clock optimize duration in **seconds**.
    pub optimize_duration: f64,
    /// Recall@k, in `[0.0, 1.0]`.
    pub recall: f64,
    /// NDCG@k, in `[0.0, 1.0]`.
    pub ndcg: f64,
    /// Mean serial query latency in **milliseconds**.
    pub serial_latency_avg: f64,
    /// p50 serial query latency in **milliseconds**.
    pub serial_latency_p50: f64,
    /// p95 serial query latency in **milliseconds**.
    pub serial_latency_p95: f64,
    /// p99 serial query latency in **milliseconds**.
    pub serial_latency_p99: f64,
    /// Total queries executed during the recall phase.
    pub serial_query_count: u64,
    /// Concurrent QPS values per concurrency level (Phase 2; empty for now).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conc_qps_list: Vec<f64>,
    /// Concurrent p99 latency per level in **milliseconds** (Phase 2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conc_latency_p99_list: Vec<f64>,
}

/// Run timestamps and host snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timestamps {
    /// Wall-clock start time, RFC 3339 string.
    pub started_at: String,
    /// Wall-clock end time, RFC 3339 string.
    pub finished_at: String,
    /// Host snapshot.
    pub host: HostInfo,
}

impl TestResult {
    /// Pretty-print to JSON, suitable for `results/.../*.json`.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}
