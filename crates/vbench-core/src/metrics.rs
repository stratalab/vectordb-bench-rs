//! Recall and latency metrics.
//!
//! Two functions and one wrapper:
//!
//! - [`recall_at_k`] — fraction of the ground-truth top-k that the adapter's
//!   top-k actually returned. Values in `[0.0, 1.0]`.
//! - [`ndcg_at_k`] — Normalized Discounted Cumulative Gain at k. Values in
//!   `[0.0, 1.0]`. Rewards correct results that appear earlier in the result
//!   list, unlike `recall_at_k` which is rank-insensitive.
//! - [`LatencyHistogram`] — thin wrapper around `hdrhistogram::Histogram<u64>`
//!   pre-configured for vbench's latency range (1 µs..60 s, 3 sig figs).

use std::collections::HashSet;

use hdrhistogram::Histogram;

use crate::error::Result;

/// Compute Recall@K.
///
/// `actual` is the adapter's returned top-k (in score order, best first).
/// `ground_truth` is the dataset's reference neighbour list — typically
/// longer than k. We compare `actual[..k]` against `ground_truth[..k]`
/// (or fewer, if either list is shorter than k).
///
/// Returns the fraction of ground-truth top-k ids that appear in
/// `actual[..k]`. Returns `0.0` for `k == 0` to avoid divide-by-zero.
///
/// # Panics
///
/// Never panics.
pub fn recall_at_k(actual: &[u64], ground_truth: &[u64], k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let truth_k = k.min(ground_truth.len());
    if truth_k == 0 {
        return 0.0;
    }
    let actual_top: HashSet<u64> = actual.iter().take(k).copied().collect();
    let hits = ground_truth
        .iter()
        .take(truth_k)
        .filter(|id| actual_top.contains(id))
        .count();
    hits as f64 / truth_k as f64
}

/// Compute NDCG@K.
///
/// Discounts gain by `1 / log2(rank + 2)`. Each ground-truth id earns a
/// gain of 1.0 if found anywhere in `actual[..k]`. The result is normalised
/// against the ideal DCG, which assumes every ground-truth id appears in the
/// top-k positions in the same order they were given.
///
/// This matches VectorDBBench's NDCG implementation.
pub fn ndcg_at_k(actual: &[u64], ground_truth: &[u64], k: usize) -> f64 {
    if k == 0 {
        return 0.0;
    }
    let truth_k = k.min(ground_truth.len());
    if truth_k == 0 {
        return 0.0;
    }
    let truth_set: HashSet<u64> = ground_truth.iter().take(truth_k).copied().collect();

    let dcg: f64 = actual
        .iter()
        .take(k)
        .enumerate()
        .filter(|(_, id)| truth_set.contains(id))
        .map(|(rank, _)| 1.0 / ((rank + 2) as f64).log2())
        .sum();

    // Ideal DCG: every relevant item in the top positions.
    let idcg: f64 = (0..truth_k)
        .map(|rank| 1.0 / ((rank + 2) as f64).log2())
        .sum();

    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
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

    /// Mean latency (microseconds).
    pub fn mean_micros(&self) -> f64 {
        self.inner.mean()
    }

    /// Latency at the given percentile (0.0..=100.0), in microseconds.
    pub fn percentile_micros(&self, p: f64) -> u64 {
        self.inner.value_at_quantile(p / 100.0)
    }

    /// Convenience: p50 in milliseconds.
    pub fn p50_ms(&self) -> f64 {
        self.percentile_micros(50.0) as f64 / 1000.0
    }

    /// Convenience: p95 in milliseconds.
    pub fn p95_ms(&self) -> f64 {
        self.percentile_micros(95.0) as f64 / 1000.0
    }

    /// Convenience: p99 in milliseconds.
    pub fn p99_ms(&self) -> f64 {
        self.percentile_micros(99.0) as f64 / 1000.0
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
            .field("mean_us", &self.mean_micros())
            .field("p50_ms", &self.p50_ms())
            .field("p95_ms", &self.p95_ms())
            .field("p99_ms", &self.p99_ms())
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
