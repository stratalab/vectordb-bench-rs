//! `TestResult` schema — strict wire-compatible with VectorDBBench upstream.
//!
//! Mirrors `vectordb_bench/models.py:TestResult` and `vectordb_bench/metric.py:Metric`
//! field-for-field, including units. The published JSON drops cleanly into
//! the existing leaderboard tooling without translation.
//!
//! ## Structure
//!
//! ```text
//! TestResult                          # container; one per benchmark run
//!   run_id        : String (UUID4 hex, no dashes)
//!   task_label    : String
//!   results       : [CaseResult]      # one per (DB, case) combination
//!   file_fmt      : String            # filename template, kept for parity
//!   timestamp     : f64               # unix epoch (seconds)
//!
//! CaseResult                          # one DB-x-case run
//!   metrics       : Metric            # the numbers
//!   task_config   : TaskConfig        # adapter, dataset, knobs
//!   label         : String            # ":)" / "x" / "?"
//! ```
//!
//! ## Units (CRITICAL — getting this wrong silently breaks comparability)
//!
//! Verified against published results in
//! `vectordb_bench/results/ElasticCloud/result_20260403_standard_elasticcloud.json`:
//!
//! | Field | Unit |
//! |---|---|
//! | `insert_duration`, `optimize_duration`, `load_duration` | **seconds** (f64) |
//! | `qps` | queries / second (f64) |
//! | `serial_latency_p99`, `serial_latency_p95` | **seconds** (f64), e.g. `0.0106` = 10.6 ms |
//! | `conc_latency_*_list` | **seconds** (Vec<f64>) |
//! | `recall`, `ndcg` | `[0.0, 1.0]` (f64) |
//! | `load_duration` invariant | == `insert_duration + optimize_duration` |
//!
//! ## What we leave at defaults
//!
//! Phase 1 only runs the serial-search phase, so every concurrent and
//! streaming field stays at its zero / empty default. The schema fields
//! are still present (upstream's tooling expects them), just empty.

use serde::{Deserialize, Serialize};

/// Top-level result document (strict upstream parity).
///
/// Phase 1 always emits a `results` array of length 1 (we run one DB on one
/// dataset per invocation), but the container shape matches upstream's
/// `TestResult` so multi-DB orchestrators could append to `results` later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// UUID4 hex (no dashes), e.g. `c11e83b51ff14060a08f06d58f801214`.
    /// Groups all `CaseResult`s from one orchestrated run.
    pub run_id: String,
    /// Free-form label, surfaced verbatim by leaderboard tooling.
    pub task_label: String,
    /// Per-case results. Length 1 in Phase 1.
    pub results: Vec<CaseResult>,
    /// Filename template, kept for parity with upstream's struct. We don't
    /// actually use this — vbench-cli writes to the path the user passes.
    /// Default value matches upstream's `"result_{}_{}_{}.json"`.
    #[serde(default = "default_file_fmt")]
    pub file_fmt: String,
    /// Unix epoch in seconds (matches upstream's float seconds).
    pub timestamp: f64,
}

fn default_file_fmt() -> String {
    "result_{}_{}_{}.json".to_string()
}

/// One DB-x-case result. Wraps the metric numbers, the task configuration
/// that produced them, and a status label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    /// The numbers.
    pub metrics: Metric,
    /// What was run, against what, with which knobs.
    pub task_config: TaskConfig,
    /// `":)" / "x" / "?"` — success / failure / out-of-range.
    pub label: String,
}

/// The numbers. Mirrors `vectordb_bench/metric.py:Metric` exactly, including
/// every field upstream's leaderboard tooling reads.
///
/// **All durations and latencies are in seconds.**
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metric {
    // ---- load cases ----
    /// Maximum vectors loaded before OOM (capacity cases). Default 0.
    pub max_load_count: u64,

    // ---- performance and streaming cases ----
    /// Pure-insert wall clock (seconds).
    pub insert_duration: f64,
    /// Optimize / index-build wall clock (seconds).
    pub optimize_duration: f64,
    /// `insert_duration + optimize_duration` (seconds).
    pub load_duration: f64,

    // ---- performance cases ----
    /// Max QPS observed during the concurrent phase. `0.0` for serial-only
    /// runs (matches upstream's behaviour).
    pub qps: f64,
    /// Serial-phase p99 latency (seconds).
    pub serial_latency_p99: f64,
    /// Serial-phase p95 latency (seconds).
    pub serial_latency_p95: f64,
    /// Recall@k.
    pub recall: f64,
    /// NDCG@k (upstream's variant — discount uses index-in-ground-truth).
    pub ndcg: f64,

    // ---- concurrent (Phase 2) ----
    /// Concurrency levels swept (e.g. `[1, 5, 10, 20, 30, 40, 60, 80]`).
    pub conc_num_list: Vec<i32>,
    /// QPS per concurrency level.
    pub conc_qps_list: Vec<f64>,
    /// p99 latency per concurrency level (seconds).
    pub conc_latency_p99_list: Vec<f64>,
    /// p95 latency per concurrency level (seconds).
    pub conc_latency_p95_list: Vec<f64>,
    /// Mean latency per concurrency level (seconds).
    pub conc_latency_avg_list: Vec<f64>,

    // ---- streaming (Phase 3+) ----
    /// Streaming case: ideal insert duration.
    pub st_ideal_insert_duration: i64,
    /// Streaming case: search stages.
    pub st_search_stage_list: Vec<i64>,
    /// Streaming case: search times.
    pub st_search_time_list: Vec<f64>,
    /// Streaming case: max QPS list-of-lists.
    pub st_max_qps_list_list: Vec<Vec<f64>>,
    /// Streaming case: per-stage recall.
    pub st_recall_list: Vec<f64>,
    /// Streaming case: per-stage NDCG.
    pub st_ndcg_list: Vec<f64>,
    /// Streaming case: per-stage p99 serial latency (seconds).
    pub st_serial_latency_p99_list: Vec<f64>,
    /// Streaming case: per-stage p95 serial latency (seconds).
    pub st_serial_latency_p95_list: Vec<f64>,
    /// Streaming case: per-stage concurrent failure rate.
    pub st_conc_failed_rate_list: Vec<f64>,
    /// Streaming case: per-stage concurrency level lists.
    pub st_conc_num_list_list: Vec<Vec<i32>>,
    /// Streaming case: per-stage concurrent QPS lists.
    pub st_conc_qps_list_list: Vec<Vec<f64>>,
    /// Streaming case: per-stage concurrent p99 latency lists (seconds).
    pub st_conc_latency_p99_list_list: Vec<Vec<f64>>,
    /// Streaming case: per-stage concurrent p95 latency lists (seconds).
    pub st_conc_latency_p95_list_list: Vec<Vec<f64>>,
    /// Streaming case: per-stage concurrent average latency lists (seconds).
    pub st_conc_latency_avg_list_list: Vec<Vec<f64>>,
}

/// Task configuration: which DB, which case, which stages, which tuning.
///
/// Mirrors `vectordb_bench/models.py:TaskConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// DB identifier (e.g. "Strata", "Milvus", "Qdrant"). Upstream uses an
    /// enum here; we use a string so adapters can declare names that don't
    /// exist in upstream's enum yet.
    pub db: String,
    /// Adapter-specific connection / credential bag. Captured opaquely.
    pub db_config: serde_json::Value,
    /// Adapter-specific tuning bag (e.g. `{"M": 16, "efConstruction": 200}`).
    /// Captured opaquely.
    pub db_case_config: serde_json::Value,
    /// Case identification (case_id, k, custom_case, concurrency search config).
    pub case_config: CaseConfig,
    /// Phases run, e.g. `["drop_old", "load", "search_serial"]`.
    pub stages: Vec<String>,
    /// Concurrent loaders during the load phase. Default 1.
    pub load_concurrency: i32,
}

/// Case configuration. Mirrors `vectordb_bench/models.py:CaseConfig`.
///
/// `case_id` is an integer enum upstream — see
/// `vectordb_bench/backend/cases.py:CaseType`. Cohere-1M = `5`
/// (Performance768D1M).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseConfig {
    /// Upstream `CaseType` integer enum value.
    pub case_id: i32,
    /// Custom-case parameters when `case_id` indicates a custom case
    /// (`100` or `101`); otherwise typically empty.
    pub custom_case: Option<serde_json::Value>,
    /// k for recall@k and ndcg@k. Upstream defaults to 100.
    pub k: i32,
    /// Concurrency sweep configuration (Phase 2). The schema requires the
    /// field to be present even when no concurrent phase runs.
    pub concurrency_search_config: ConcurrencySearchConfig,
}

/// Concurrency sweep configuration. Mirrors
/// `vectordb_bench/models.py:ConcurrencySearchConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencySearchConfig {
    /// Concurrency levels to sweep, e.g. `[1, 5, 10, 20, 30, 40, 60, 80]`.
    pub num_concurrency: Vec<i32>,
    /// Wall-clock seconds per concurrency level.
    pub concurrency_duration: i32,
    /// Hard timeout per concurrency level.
    pub concurrency_timeout: i32,
}

impl Default for ConcurrencySearchConfig {
    /// Defaults match upstream's `vectordb_bench/__init__.py` constants.
    fn default() -> Self {
        Self {
            num_concurrency: vec![1, 5, 10, 20, 30, 40, 60, 80],
            concurrency_duration: 30,
            concurrency_timeout: 3600,
        }
    }
}

/// Result label values matching upstream's `ResultLabel`.
pub mod result_label {
    /// Successful run.
    pub const NORMAL: &str = ":)";
    /// Failed run.
    pub const FAILED: &str = "x";
    /// Numbers fall outside the expected range.
    pub const OUT_OF_RANGE: &str = "?";
}

impl TestResult {
    /// Pretty-print to JSON, suitable for `results/.../*.json`.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Generate a fresh run_id matching upstream's UUID4-hex format
    /// (32 hex chars, no dashes).
    pub fn new_run_id() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }
}
