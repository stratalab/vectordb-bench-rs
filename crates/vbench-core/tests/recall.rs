//! Unit tests for `recall_at_k` and `ndcg_at_k`.
//!
//! These tests verify wire-compatibility with VectorDBBench upstream's
//! `vectordb_bench/metric.py:calc_recall` and `calc_ndcg`. Both functions
//! assume the caller has pre-truncated `ground_truth` to length k
//! (matching upstream's `gt[: self.k]` slice).
//!
//! Note on NDCG: upstream's `calc_ndcg` is **insensitive to the order of
//! ids within `actual`** — the discount uses the index in `ground_truth`,
//! not the index in `actual`. The `ndcg_order_insensitive` test below
//! pins this surprising-but-deliberate behaviour.

use vbench_core::{ideal_dcg_at_k, ndcg_at_k, recall_at_k};

// =============================================================================
// recall_at_k
// =============================================================================

#[test]
fn recall_perfect_match_is_one() {
    let actual = vec![1, 2, 3, 4, 5];
    let truth = vec![1, 2, 3, 4, 5];
    assert_eq!(recall_at_k(&actual, &truth, 5), 1.0);
}

#[test]
fn recall_perfect_match_with_extra_actual_truncated_at_k() {
    // Adapter returned more than k; we only consider the first k.
    let actual = vec![1, 2, 3, 4, 5, 999];
    let truth = vec![1, 2, 3, 4, 5];
    assert_eq!(recall_at_k(&actual, &truth, 5), 1.0);
}

#[test]
fn recall_zero_overlap_is_zero() {
    let actual = vec![10, 20, 30];
    let truth = vec![1, 2, 3];
    assert_eq!(recall_at_k(&actual, &truth, 3), 0.0);
}

#[test]
fn recall_partial_overlap_divides_by_k_constant() {
    // 3 of 5 returned ids are in truth.
    // Upstream's formula: hits / k = 3/5 = 0.6
    let actual = vec![1, 2, 3, 99, 100];
    let truth = vec![1, 2, 3, 4, 5];
    assert!((recall_at_k(&actual, &truth, 5) - 0.6).abs() < 1e-9);
}

#[test]
fn recall_actual_shorter_than_k_still_divides_by_k() {
    // Upstream's `calc_recall` does `np.zeros(count)` then iterates `got`.
    // If got has fewer than k entries, the trailing recalls stay 0 and the
    // mean still divides by k. So the result is hits / k, not hits / len(got).
    let actual = vec![1, 2]; // length 2
    let truth = vec![1, 2, 3, 4, 5];
    // hits = 2, k = 5, expected = 0.4
    assert!((recall_at_k(&actual, &truth, 5) - 0.4).abs() < 1e-9);
}

#[test]
fn recall_order_insensitive() {
    let actual_a = vec![5, 4, 3, 2, 1];
    let actual_b = vec![1, 2, 3, 4, 5];
    let truth = vec![1, 2, 3, 4, 5];
    assert_eq!(
        recall_at_k(&actual_a, &truth, 5),
        recall_at_k(&actual_b, &truth, 5)
    );
}

#[test]
fn recall_k_zero_is_zero() {
    let actual = vec![1, 2, 3];
    let truth = vec![1, 2, 3];
    assert_eq!(recall_at_k(&actual, &truth, 0), 0.0);
}

#[test]
fn recall_empty_actual_is_zero() {
    let actual: Vec<u64> = vec![];
    let truth = vec![1, 2, 3];
    assert_eq!(recall_at_k(&actual, &truth, 3), 0.0);
}

#[test]
fn recall_empty_truth_is_zero() {
    let actual = vec![1, 2, 3];
    let truth: Vec<u64> = vec![];
    assert_eq!(recall_at_k(&actual, &truth, 3), 0.0);
}

// =============================================================================
// ndcg_at_k
// =============================================================================

#[test]
fn ndcg_perfect_match_is_one() {
    let actual = vec![1, 2, 3, 4, 5];
    let truth = vec![1, 2, 3, 4, 5];
    assert!((ndcg_at_k(&actual, &truth, 5) - 1.0).abs() < 1e-9);
}

#[test]
fn ndcg_zero_overlap_is_zero() {
    let actual = vec![10, 20, 30];
    let truth = vec![1, 2, 3];
    assert_eq!(ndcg_at_k(&actual, &truth, 3), 0.0);
}

#[test]
fn ndcg_order_insensitive() {
    // CRITICAL: upstream's calc_ndcg discounts by ground_truth position,
    // not actual position. So getting all the right ids back in any order
    // produces the same NDCG. This is unusual but it's what the leaderboard
    // expects.
    let truth = vec![1, 2, 3, 4, 5];
    let in_order = vec![1, 2, 3, 4, 5];
    let reversed = vec![5, 4, 3, 2, 1];
    let shuffled = vec![3, 5, 1, 4, 2];

    let a = ndcg_at_k(&in_order, &truth, 5);
    let b = ndcg_at_k(&reversed, &truth, 5);
    let c = ndcg_at_k(&shuffled, &truth, 5);

    assert!((a - b).abs() < 1e-9, "in_order vs reversed: {a} vs {b}");
    assert!((a - c).abs() < 1e-9, "in_order vs shuffled: {a} vs {c}");
    assert!((a - 1.0).abs() < 1e-9);
}

#[test]
fn ndcg_partial_overlap_uses_truth_position_for_discount() {
    // Truth: [10, 20, 30, 40, 50] — id 10 has the best position (0).
    // Actual returns just [50, 10] (k=5). Both are in truth.
    // Upstream's formula:
    //   ideal_dcg = 1/log2(2) + 1/log2(3) + 1/log2(4) + 1/log2(5) + 1/log2(6)
    //   For id 50, idx in truth = 4 → discount = 1/log2(6)
    //   For id 10, idx in truth = 0 → discount = 1/log2(2)
    //   dcg = 1/log2(6) + 1/log2(2)
    //   ndcg = dcg / ideal_dcg
    let truth = vec![10, 20, 30, 40, 50];
    let actual = vec![50, 10];
    let expected_dcg = 1.0 / 6f64.log2() + 1.0 / 2f64.log2();
    let expected_idcg = ideal_dcg_at_k(5);
    let expected_ndcg = expected_dcg / expected_idcg;

    let got = ndcg_at_k(&actual, &truth, 5);
    assert!(
        (got - expected_ndcg).abs() < 1e-9,
        "got {got}, expected {expected_ndcg}"
    );
}

#[test]
fn ndcg_dedupes_actual() {
    // Upstream uses `set(got)` so duplicates in `actual` are counted once.
    let truth = vec![1, 2, 3, 4, 5];
    let actual_unique = vec![1];
    let actual_dup = vec![1, 1, 1, 1, 1];
    assert_eq!(
        ndcg_at_k(&actual_unique, &truth, 5),
        ndcg_at_k(&actual_dup, &truth, 5)
    );
}

#[test]
fn ndcg_k_zero_is_zero() {
    let actual = vec![1, 2, 3];
    let truth = vec![1, 2, 3];
    assert_eq!(ndcg_at_k(&actual, &truth, 0), 0.0);
}

#[test]
fn ndcg_value_in_unit_interval() {
    let actual = vec![3, 1, 99, 2, 100];
    let truth = vec![1, 2, 3, 4, 5];
    let n = ndcg_at_k(&actual, &truth, 5);
    assert!((0.0..=1.0).contains(&n), "ndcg out of range: {n}");
}

#[test]
fn ideal_dcg_matches_python_reference() {
    // Reference value from upstream's `get_ideal_dcg(5)`:
    //   sum(1/log2(i+2) for i in range(5))
    //   = 1/log2(2) + 1/log2(3) + 1/log2(4) + 1/log2(5) + 1/log2(6)
    //   ≈ 1.0 + 0.6309 + 0.5 + 0.4307 + 0.3869
    //   ≈ 2.9485
    let got = ideal_dcg_at_k(5);
    assert!(
        (got - 2.9485).abs() < 1e-3,
        "ideal_dcg(5) = {got}, expected ~2.9485"
    );
}
