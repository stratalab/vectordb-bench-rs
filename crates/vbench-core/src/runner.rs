//! Phase orchestration: drives a [`BenchAdapter`] through load → optimize →
//! recall → serial-latency → produce a [`TestResult`].
//!
//! Phase 1 runs the phases serially. Phase 2 will add a concurrent QPS
//! sweep that fans out across multiple adapter clones.
//!
//! ## Why a free function instead of a `Runner` struct
//!
//! `run_benchmark` borrows the dataset and consumes the adapter. There's no
//! state worth carrying across runs — each call is independent — so a
//! struct would just be a bag of parameters. The free function makes the
//! ownership story explicit at the call site.

use std::time::Instant;

use chrono::Utc;
use tracing::info;

use crate::adapter::{BenchAdapter, VectorRow};
use crate::dataset::LoadedDataset;
use crate::error::{Result, VbenchError};
use crate::host::HostInfo;
use crate::metrics::{ndcg_at_k, recall_at_k, LatencyHistogram};
use crate::result::{CaseConfig, DbConfig, ResultMetrics, TaskConfig, TestResult, Timestamps};

/// Phase knobs that the CLI exposes.
///
/// Defaults match VectorDBBench's standard configuration where applicable
/// (`recall_k = 10`, `warmup_queries = 200`, `batch_size = 1000`).
#[derive(Debug, Clone)]
pub struct RunnerOptions {
    /// Rows per `BenchAdapter::load` call.
    pub batch_size: usize,
    /// k for recall@k and ndcg@k.
    pub recall_k: usize,
    /// Warm-up queries to issue during the optimize phase.
    pub warmup_queries: usize,
    /// Free-form label for the published result.
    pub task_label: String,
    /// `db_config.install_method` (e.g. "stratadb.org/install.sh").
    pub install_method: Option<String>,
    /// Free-form notes for `db_config.notes`.
    pub db_notes: Option<String>,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            recall_k: 10,
            warmup_queries: 200,
            task_label: "vbench-run".to_string(),
            install_method: None,
            db_notes: None,
        }
    }
}

/// Drive an adapter through every Phase 1 phase and produce a [`TestResult`].
///
/// Phases:
///
/// 1. **load** — stream training rows into the adapter in `batch_size` chunks
/// 2. **count** — assert the adapter holds `num_train` rows post-load
/// 3. **optimize** — call `BenchAdapter::optimize` then issue `warmup_queries`
///    real test queries (discarded; just to force any lazy index build)
/// 4. **recall + serial latency** — serial loop over every test query,
///    measure per-query latency, accumulate recall@k and ndcg@k
///
/// Errors are wrapped via [`VbenchError::Adapter`] so the caller doesn't need
/// to know which adapter produced them.
pub async fn run_benchmark<A: BenchAdapter>(
    adapter: A,
    dataset: &LoadedDataset,
    opts: &RunnerOptions,
) -> Result<TestResult> {
    let started_at = Utc::now().to_rfc3339();
    let info = adapter.info();
    let metric_str = match dataset.spec.metric {
        crate::Metric::Cosine => "cosine",
        crate::Metric::L2 => "l2",
        crate::Metric::Ip => "ip",
    };

    // -------- Phase 1: load --------
    info!(
        dataset = dataset.spec.id,
        num_train = dataset.spec.num_train,
        batch_size = opts.batch_size,
        "load phase starting"
    );
    let load_start = Instant::now();
    let mut batch: Vec<VectorRow<'_>> = Vec::with_capacity(opts.batch_size);
    for (id, vector) in dataset.train_iter() {
        batch.push(VectorRow {
            id,
            vector,
            labels: None,
        });
        if batch.len() == opts.batch_size {
            adapter
                .load(&batch)
                .await
                .map_err(|e| VbenchError::Adapter(e.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        adapter
            .load(&batch)
            .await
            .map_err(|e| VbenchError::Adapter(e.to_string()))?;
        batch.clear();
    }
    let load_duration = load_start.elapsed().as_secs_f64();
    info!(secs = load_duration, "load phase complete");

    // -------- count sanity check --------
    let count = adapter
        .count()
        .await
        .map_err(|e| VbenchError::Adapter(e.to_string()))?;
    if count as usize != dataset.spec.num_train {
        return Err(VbenchError::InvalidInput(format!(
            "post-load count mismatch: adapter has {} rows, expected {}",
            count, dataset.spec.num_train
        )));
    }

    // -------- Phase 2: optimize + warmup --------
    info!(
        warmup_queries = opts.warmup_queries,
        "optimize phase starting"
    );
    let optimize_start = Instant::now();
    adapter
        .optimize()
        .await
        .map_err(|e| VbenchError::Adapter(e.to_string()))?;
    for (_, query) in dataset.test_iter().take(opts.warmup_queries) {
        // Discard results; we only care about the side effect of warming up.
        let _ = adapter.search(query, opts.recall_k).await;
    }
    let optimize_duration = optimize_start.elapsed().as_secs_f64();
    info!(secs = optimize_duration, "optimize phase complete");

    // -------- Phase 3: recall + serial latency --------
    info!(
        num_test = dataset.spec.num_test,
        recall_k = opts.recall_k,
        "recall phase starting"
    );
    let mut hist = LatencyHistogram::new()?;
    let mut sum_recall = 0.0;
    let mut sum_ndcg = 0.0;
    let mut query_count: u64 = 0;

    for (i, query) in dataset.test_iter() {
        let q_start = Instant::now();
        let actual = adapter
            .search(query, opts.recall_k)
            .await
            .map_err(|e| VbenchError::Adapter(e.to_string()))?;
        let q_micros = q_start.elapsed().as_micros() as u64;
        hist.record_micros(q_micros)?;

        let truth = dataset.ground_truth_for(i);
        sum_recall += recall_at_k(&actual, truth, opts.recall_k);
        sum_ndcg += ndcg_at_k(&actual, truth, opts.recall_k);
        query_count += 1;
    }

    let avg_recall = if query_count > 0 {
        sum_recall / query_count as f64
    } else {
        0.0
    };
    let avg_ndcg = if query_count > 0 {
        sum_ndcg / query_count as f64
    } else {
        0.0
    };
    let serial_latency_avg = hist.mean_micros() / 1000.0;
    let serial_latency_p50 = hist.p50_ms();
    let serial_latency_p95 = hist.p95_ms();
    let serial_latency_p99 = hist.p99_ms();
    info!(
        recall = avg_recall,
        ndcg = avg_ndcg,
        p99_ms = serial_latency_p99,
        "recall phase complete"
    );

    // Drop the adapter cleanly. We do this before building the result so any
    // shutdown error surfaces as an adapter error.
    adapter
        .shutdown()
        .await
        .map_err(|e| VbenchError::Adapter(e.to_string()))?;

    let finished_at = Utc::now().to_rfc3339();

    Ok(TestResult {
        vbench_schema_version: "1".to_string(),
        task_label: opts.task_label.clone(),
        db_config: DbConfig {
            adapter: info.name,
            db_version: info.db_version,
            install_method: opts.install_method.clone(),
            hnsw_m: None,
            hnsw_ef_construction: None,
            hnsw_ef_search: None,
            notes: opts.db_notes.clone().or(info.notes),
        },
        case_config: CaseConfig {
            dataset: dataset.spec.id.to_string(),
            dim: dataset.spec.dim,
            metric: metric_str.to_string(),
            recall_k: opts.recall_k,
            num_train: dataset.spec.num_train,
            num_test: dataset.spec.num_test,
        },
        task_config: TaskConfig {
            batch_size: opts.batch_size,
            warmup_queries: opts.warmup_queries,
            run_concurrent: false,
        },
        metrics: ResultMetrics {
            load_duration,
            optimize_duration,
            recall: avg_recall,
            ndcg: avg_ndcg,
            serial_latency_avg,
            serial_latency_p50,
            serial_latency_p95,
            serial_latency_p99,
            serial_query_count: query_count,
            conc_qps_list: vec![],
            conc_latency_p99_list: vec![],
        },
        timestamps: Timestamps {
            started_at,
            finished_at,
            host: HostInfo::snapshot(),
        },
    })
}
