//! Phase orchestration: drives a [`BenchAdapter`] through insert → optimize
//! → recall → produce a [`TestResult`] in the strict upstream-compatible
//! schema.
//!
//! Phase 1 runs the serial-search phase only. Phase 2 will add a concurrent
//! QPS sweep that fans out across multiple adapter clones (tracked in the
//! repo issue tracker).

use std::time::Instant;

use chrono::Utc;
use tracing::info;

use crate::adapter::{BenchAdapter, VectorRow};
use crate::dataset::LoadedDataset;
use crate::error::{Result, VbenchError};
use crate::metrics::{ndcg_at_k, recall_at_k, LatencyHistogram};
use crate::result::{
    result_label, CaseConfig, CaseResult, ConcurrencySearchConfig, Metric, TaskConfig, TestResult,
};

/// Phase knobs that the CLI exposes.
///
/// Defaults match VectorDBBench upstream:
/// - `recall_k = 100` (upstream's `K_DEFAULT`)
/// - `warmup_queries = 200` (vbench-specific; upstream uses different
///   warm-up strategies per adapter)
/// - `batch_size = 1000` (vbench-specific)
#[derive(Debug, Clone)]
pub struct RunnerOptions {
    /// Rows per `BenchAdapter::load` call.
    pub batch_size: usize,
    /// k for recall@k and ndcg@k. Defaults to upstream's `K_DEFAULT = 100`.
    pub recall_k: usize,
    /// Warm-up queries to issue during the optimize phase.
    pub warmup_queries: usize,
    /// Free-form label for the published result.
    pub task_label: String,
    /// `db_config.note` (free-form, e.g. "stratadb.org/install.sh", commit
    /// SHA, …).
    pub db_note: Option<String>,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            recall_k: 100,
            warmup_queries: 200,
            task_label: "vbench-run".to_string(),
            db_note: None,
        }
    }
}

/// Drive an adapter through the Phase 1 phases and produce a [`TestResult`].
///
/// Phases:
///
/// 1. **insert** — stream training rows into the adapter in `batch_size`
///    chunks. Records `insert_duration`.
/// 2. **count** — assert the adapter holds `num_train` rows post-insert.
/// 3. **optimize** — call `BenchAdapter::optimize`, then issue
///    `warmup_queries` real test queries (results discarded; just to
///    force any lazy index build). Records `optimize_duration`.
///    `load_duration = insert_duration + optimize_duration`.
/// 4. **recall + serial latency** — serial loop over every test query,
///    measure per-query latency, accumulate `recall@k` and `ndcg@k`.
///
/// All durations and latencies in the emitted `TestResult` are in
/// **seconds** (matching upstream).
pub async fn run_benchmark<A: BenchAdapter>(
    adapter: A,
    dataset: &LoadedDataset,
    opts: &RunnerOptions,
) -> Result<TestResult> {
    let started_at_secs = Utc::now().timestamp() as f64;
    let info = adapter.info();

    // -------- Phase 1: insert --------
    info!(
        dataset = dataset.spec.id,
        num_train = dataset.spec.num_train,
        batch_size = opts.batch_size,
        "insert phase starting"
    );
    let insert_start = Instant::now();
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
    let insert_duration = insert_start.elapsed().as_secs_f64();
    info!(secs = insert_duration, "insert phase complete");

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
    let load_duration = insert_duration + optimize_duration;
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

        // Truncate ground truth to k, matching upstream's `gt[: self.k]`.
        let gt_full = dataset.ground_truth_for(i);
        let gt_topk = &gt_full[..opts.recall_k.min(gt_full.len())];

        sum_recall += recall_at_k(&actual, gt_topk, opts.recall_k);
        sum_ndcg += ndcg_at_k(&actual, gt_topk, opts.recall_k);
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
    let serial_latency_p99 = hist.p99_seconds();
    let serial_latency_p95 = hist.p95_seconds();
    info!(
        recall = avg_recall,
        ndcg = avg_ndcg,
        p99_s = serial_latency_p99,
        p95_s = serial_latency_p95,
        mean_s = hist.mean_seconds(),
        "recall phase complete"
    );

    // Drop the adapter cleanly. Shutdown errors surface as adapter errors.
    adapter
        .shutdown()
        .await
        .map_err(|e| VbenchError::Adapter(e.to_string()))?;

    // Build the strict upstream-compatible TestResult.
    let metrics = Metric {
        max_load_count: 0,
        insert_duration,
        optimize_duration,
        load_duration,
        qps: 0.0, // Phase 2 will set this from the concurrent sweep
        serial_latency_p99,
        serial_latency_p95,
        recall: avg_recall,
        ndcg: avg_ndcg,
        ..Metric::default()
    };

    // Adapter db_config and db_case_config bags. Adapters describe
    // themselves via `AdapterInfo`; the runner converts that into the
    // upstream-shaped fields.
    let db_config = serde_json::json!({
        "db_label": info.name,
        "version": info.db_version,
        "note": opts.db_note.clone().or(info.notes.clone()).unwrap_or_default(),
    });
    let db_case_config = serde_json::json!({
        "metric_type": match dataset.spec.metric {
            crate::Metric::Cosine => "COSINE",
            crate::Metric::L2 => "L2",
            crate::Metric::Ip => "IP",
        },
    });

    let case_result = CaseResult {
        metrics,
        task_config: TaskConfig {
            db: info.name.clone(),
            db_config,
            db_case_config,
            case_config: CaseConfig {
                case_id: dataset.spec.case_id,
                custom_case: None,
                k: opts.recall_k as i32,
                concurrency_search_config: ConcurrencySearchConfig::default(),
            },
            stages: vec![
                "drop_old".to_string(),
                "load".to_string(),
                "search_serial".to_string(),
            ],
            load_concurrency: 1,
        },
        label: result_label::NORMAL.to_string(),
    };

    Ok(TestResult {
        run_id: TestResult::new_run_id(),
        task_label: opts.task_label.clone(),
        results: vec![case_result],
        file_fmt: "result_{}_{}_{}.json".to_string(),
        timestamp: started_at_secs,
    })
}
