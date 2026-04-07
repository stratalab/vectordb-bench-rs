//! End-to-end runner test against an in-memory mock adapter.
//!
//! Verifies that:
//! - the load phase calls `adapter.load` with the right number of rows
//!   (in batches of the configured size)
//! - the optimize phase issues exactly `warmup_queries` queries
//! - the recall phase issues one query per test row, recall@k is computed
//!   correctly, and the resulting `TestResult` carries the right counts
//! - `shutdown` is called

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use vbench_core::{
    run_benchmark, AdapterInfo, BenchAdapter, DatasetSpec, LoadedDataset, Metric, RunnerOptions,
    VectorRow,
};

#[derive(Default)]
struct CountingAdapterState {
    load_calls: AtomicUsize,
    rows_loaded: AtomicUsize,
    optimize_calls: AtomicUsize,
    search_calls: AtomicUsize,
    shutdown_calls: AtomicUsize,
}

struct CountingAdapter {
    state: Arc<CountingAdapterState>,
    /// Canned ground-truth for each query, indexed by axis (the position
    /// of the 1.0 in the query vector). The test uses unit vectors so
    /// `query[axis] == 1.0` uniquely identifies the query, regardless of
    /// whether the runner is in the warmup or recall phase.
    canned_by_axis: Arc<Vec<Vec<u64>>>,
}

#[async_trait]
impl BenchAdapter for CountingAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: "counting".to_string(),
            db_version: "0.0.0".to_string(),
            notes: Some("test mock".to_string()),
        }
    }

    async fn open(
        _workdir: &Path,
        _dim: usize,
        _metric: Metric,
        _params: &serde_json::Value,
    ) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        // The runner test constructs the adapter directly, not via open.
        unimplemented!("CountingAdapter::open not used in this test")
    }

    async fn load(&self, rows: &[VectorRow<'_>]) -> anyhow::Result<()> {
        self.state.load_calls.fetch_add(1, Ordering::SeqCst);
        self.state
            .rows_loaded
            .fetch_add(rows.len(), Ordering::SeqCst);
        Ok(())
    }

    async fn optimize(&self) -> anyhow::Result<()> {
        self.state.optimize_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<u64>> {
        self.state.search_calls.fetch_add(1, Ordering::SeqCst);
        // The test's queries are unit vectors; identify the query by which
        // axis is set. This is independent of call order, so warmup queries
        // and recall queries can interleave without breaking the answer.
        let axis = query.iter().position(|&v| v == 1.0).unwrap_or(0);
        let answer = self.canned_by_axis.get(axis).cloned().unwrap_or_default();
        Ok(answer.into_iter().take(k).collect())
    }

    async fn count(&self) -> anyhow::Result<u64> {
        Ok(self.state.rows_loaded.load(Ordering::SeqCst) as u64)
    }

    async fn shutdown(self) -> anyhow::Result<()> {
        self.state.shutdown_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// Tiny in-memory dataset spec for test runs.
static TINY_SPEC: DatasetSpec = DatasetSpec {
    id: "tiny",
    display_name: "tiny test dataset",
    dim: 4,
    metric: Metric::Cosine,
    num_train: 5,
    num_test: 3,
    num_neighbors: 2,
    url_base: "",
    train_file: "",
    test_file: "",
    neighbors_file: "",
    cache_subdir: "tiny",
    approx_download_bytes: 0,
};

fn tiny_dataset() -> LoadedDataset {
    // 5 distinct unit vectors in 4d.
    let train_flat = vec![
        1.0, 0.0, 0.0, 0.0, // row 0
        0.0, 1.0, 0.0, 0.0, // row 1
        0.0, 0.0, 1.0, 0.0, // row 2
        0.0, 0.0, 0.0, 1.0, // row 3
        1.0, 1.0, 0.0, 0.0, // row 4
    ];
    // 3 test queries — same as some training rows for easy ground truth.
    let test_flat = vec![
        1.0, 0.0, 0.0, 0.0, // query 0 → nearest is row 0
        0.0, 1.0, 0.0, 0.0, // query 1 → nearest is row 1
        0.0, 0.0, 1.0, 0.0, // query 2 → nearest is row 2
    ];
    // Ground truth: top-2 for each.
    let truth = vec![
        vec![0u64, 4], // query 0
        vec![1u64, 4], // query 1
        vec![2u64, 0], // query 2
    ];
    LoadedDataset::from_buffers(&TINY_SPEC, train_flat, test_flat, truth).unwrap()
}

#[tokio::test]
async fn runner_drives_all_phases_against_mock_adapter() {
    let dataset = tiny_dataset();

    // Canned ground-truth answers indexed by axis. The test queries are
    // unit vectors with axes 0, 1, 2 (matching the first three rows).
    let canned: Vec<Vec<u64>> = (0..dataset.spec.num_test)
        .map(|i| dataset.ground_truth_for(i).to_vec())
        .collect();

    let state = Arc::new(CountingAdapterState::default());
    let adapter = CountingAdapter {
        state: state.clone(),
        canned_by_axis: Arc::new(canned),
    };

    // batch_size = 2 → expect ceil(5/2) = 3 load calls.
    let opts = RunnerOptions {
        batch_size: 2,
        recall_k: 2,
        warmup_queries: 1,
        task_label: "test".to_string(),
        install_method: None,
        db_notes: None,
    };

    let result = run_benchmark(adapter, &dataset, &opts).await.unwrap();

    // Phase 1: load
    assert_eq!(state.load_calls.load(Ordering::SeqCst), 3, "load batches");
    assert_eq!(state.rows_loaded.load(Ordering::SeqCst), 5, "rows loaded");

    // Phase 2: optimize + warmup
    assert_eq!(
        state.optimize_calls.load(Ordering::SeqCst),
        1,
        "optimize called once"
    );

    // Phase 3: recall (warmup + test queries)
    let total_searches = state.search_calls.load(Ordering::SeqCst);
    assert_eq!(
        total_searches,
        opts.warmup_queries + dataset.spec.num_test,
        "warmup + test queries"
    );

    // shutdown
    assert_eq!(state.shutdown_calls.load(Ordering::SeqCst), 1, "shutdown");

    // The mock returns ground-truth answers verbatim, so recall should be 1.0.
    assert!(
        (result.metrics.recall - 1.0).abs() < 1e-9,
        "expected recall 1.0, got {}",
        result.metrics.recall
    );
    assert!(
        (result.metrics.ndcg - 1.0).abs() < 1e-9,
        "expected ndcg 1.0, got {}",
        result.metrics.ndcg
    );
    assert_eq!(result.metrics.serial_query_count, 3);
    assert_eq!(result.case_config.dataset, "tiny");
    assert_eq!(result.case_config.recall_k, 2);
    assert_eq!(result.task_config.batch_size, 2);
    assert_eq!(result.task_config.warmup_queries, 1);
    assert!(!result.task_config.run_concurrent);
}

#[tokio::test]
async fn runner_rejects_count_mismatch() {
    // Adapter that lies about its count — runner should bail.
    struct LyingAdapter {
        state: Arc<CountingAdapterState>,
    }
    #[async_trait]
    impl BenchAdapter for LyingAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                name: "lying".to_string(),
                db_version: "0.0.0".to_string(),
                notes: None,
            }
        }
        async fn open(
            _: &Path,
            _: usize,
            _: Metric,
            _: &serde_json::Value,
        ) -> anyhow::Result<Self> {
            unimplemented!()
        }
        async fn load(&self, _: &[VectorRow<'_>]) -> anyhow::Result<()> {
            self.state.load_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn search(&self, _: &[f32], _: usize) -> anyhow::Result<Vec<u64>> {
            Ok(vec![])
        }
        async fn count(&self) -> anyhow::Result<u64> {
            Ok(0) // wrong on purpose
        }
        async fn shutdown(self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let dataset = tiny_dataset();
    let state = Arc::new(CountingAdapterState::default());
    let adapter = LyingAdapter {
        state: state.clone(),
    };
    let opts = RunnerOptions::default();
    let err = run_benchmark(adapter, &dataset, &opts).await.unwrap_err();
    assert!(err.to_string().contains("post-load count mismatch"));
}
