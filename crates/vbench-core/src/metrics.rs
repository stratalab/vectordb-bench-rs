//! Recall and latency metrics.
//!
//! [`recall_at_k`] and [`ndcg_at_k`] match VectorDBBench upstream's
//! implementations exactly (verified against
//! `vectordb_bench/metric.py:calc_recall` and `calc_ndcg` and the call sites
//! in `vectordb_bench/backend/runner/serial_runner.py`). Computing these
//! ourselves with a different formula would silently make our numbers
//! incomparable to the leaderboard.
//!
//! Both functions assume `ground_truth` has been **pre-truncated to length
//! k by the caller** (matching upstream's `gt[: self.k]` slice). The
//! callers in [`crate::runner`] do this slicing.
//!
//! [`LatencyHistogram`] wraps `hdrhistogram::Histogram<u64>` pre-configured
//! for vbench's latency range (1 µs..60 s, 3 sig figs).

use std::collections::HashSet;

use hdrhistogram::Histogram;

use crate::error::Result;

/// Compute Recall@K, matching upstream's `calc_recall`.
///
/// Iterates `actual.iter().take(k)` and counts how many ids appear in
/// `ground_truth_topk` (which the caller has already truncated to length
/// k). Divides by `k` (the constant — **not** `min(k, ground_truth.len())`).
///
/// ```text
/// recall = | { actual[i] for i in 0..k : actual[i] in ground_truth_topk } | / k
/// ```
///
/// Reference: `vectordb_bench/metric.py:calc_recall` and the call site at
/// `serial_runner.py:180` — `calc_recall(self.k, gt[: self.k], results)`.
///
/// Returns 0.0 for `k == 0`.
pub fn recall_at_k(actual: &[u64], ground_truth_topk: &[u64], k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let truth_set: HashSet<u64> = ground_truth_topk.iter().copied().collect();
    let hits = actual
        .iter()
        .take(k)
        .filter(|id| truth_set.contains(id))
        .count();
    hits as f64 / k as f64
}

/// Compute NDCG@K, matching upstream's `calc_ndcg`.
///
/// **Important**: this is NOT textbook NDCG. Upstream's implementation
/// discounts each found id by its **position in `ground_truth_topk`**, not
/// its position in `actual`. As a consequence, the score is **insensitive
/// to the order** of items within `actual` — getting the right ids back in
/// any permutation produces the same NDCG.
///
/// Algorithm (verified against `vectordb_bench/metric.py:calc_ndcg`):
///
/// ```text
/// dcg = 0
/// for got_id in set(actual.iter().take(k)):
///     if got_id in ground_truth_topk:
///         idx = ground_truth_topk.index(got_id)   # position in TRUTH, not actual
///         dcg += 1 / log2(idx + 2)
/// ndcg = dcg / ideal_dcg
/// ```
///
/// where `ideal_dcg = sum(1/log2(i+2) for i in 0..k)`.
///
/// `actual` is deduplicated via a set (matching upstream's `set(got)`).
///
/// Returns 0.0 for `k == 0` or when `ideal_dcg == 0`.
pub fn ndcg_at_k(actual: &[u64], ground_truth_topk: &[u64], k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let ideal_dcg = ideal_dcg_at_k(k);
    if ideal_dcg == 0.0 {
        return 0.0;
    }

    let actual_set: HashSet<u64> = actual.iter().take(k).copied().collect();
    let mut dcg = 0.0;
    for got_id in actual_set {
        if let Some(idx) = ground_truth_topk.iter().position(|&id| id == got_id) {
            dcg += 1.0 / ((idx + 2) as f64).log2();
        }
    }
    dcg / ideal_dcg
}

/// `sum(1/log2(i+2) for i in 0..k)`. Matches upstream's `get_ideal_dcg`.
pub fn ideal_dcg_at_k(k: usize) -> f64 {
    (0..k).map(|i| 1.0 / ((i + 2) as f64).log2()).sum()
}

/// Latency histogram tuned for vbench's measurement range.
///
/// Records values in microseconds. The bounds (1 µs..60 s) are wide enough
/// to cover everything from a CPU-cached scalar lookup to a deep ANN search
/// over a 100M dataset, and 3 significant figures keeps the histogram
/// memory footprint to a few hundred KB.
pub struct LatencyHistogram {
    inner: Histogram<u64>,
}

impl LatencyHistogram {
    /// Construct an empty histogram with vbench's standard configuration:
    /// 1 µs minimum, 60 s maximum, 3 significant figures.
    pub fn new() -> Result<Self> {
        let inner = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
        Ok(Self { inner })
    }

    /// Record one latency sample (microseconds).
    pub fn record_micros(&mut self, micros: u64) -> Result<()> {
        // Saturate at the upper bound rather than failing — a single anomalous
        // 60s+ outlier shouldn't kill the whole run.
        let clamped = micros.clamp(1, 60_000_000);
        self.inner.record(clamped)?;
        Ok(())
    }

    /// Mean latency (microseconds, internal precision).
    pub fn mean_micros(&self) -> f64 {
        self.inner.mean()
    }

    /// Latency at the given percentile (0.0..=100.0), in microseconds.
    pub fn percentile_micros(&self, p: f64) -> u64 {
        self.inner.value_at_quantile(p / 100.0)
    }

    /// p50 in **seconds** (matches upstream's `serial_latency_*` unit).
    pub fn p50_seconds(&self) -> f64 {
        self.percentile_micros(50.0) as f64 / 1_000_000.0
    }

    /// p95 in **seconds**.
    pub fn p95_seconds(&self) -> f64 {
        self.percentile_micros(95.0) as f64 / 1_000_000.0
    }

    /// p99 in **seconds**.
    pub fn p99_seconds(&self) -> f64 {
        self.percentile_micros(99.0) as f64 / 1_000_000.0
    }

    /// Mean latency in **seconds**.
    pub fn mean_seconds(&self) -> f64 {
        self.inner.mean() / 1_000_000.0
    }

    /// Total samples recorded.
    pub fn count(&self) -> u64 {
        self.inner.len()
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new().expect("vbench histogram bounds are valid by construction")
    }
}

impl std::fmt::Debug for LatencyHistogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LatencyHistogram")
            .field("count", &self.count())
            .field("mean_s", &self.mean_seconds())
            .field("p50_s", &self.p50_seconds())
            .field("p95_s", &self.p95_seconds())
            .field("p99_s", &self.p99_seconds())
            .finish()
    }
}

// Reject the obvious foot-gun: returning `Result` from accessor methods.
// Keeping this guard close to the public surface so refactors don't drift.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _assert() {
        _assert_send_sync::<LatencyHistogram>();
    }
};
