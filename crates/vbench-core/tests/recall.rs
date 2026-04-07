//! Unit tests for `recall_at_k` and `ndcg_at_k`.
//!
//! Hand-built fixtures kept small enough to verify by inspection. The goal is
//! to catch off-by-one errors, bad k handling, and any drift away from the
//! `[0.0, 1.0]` invariant.

use vbench_core::{ndcg_at_k, recall_at_k};

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
fn recall_perfect_match_with_extra_actual_is_one() {
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
fn recall_partial_overlap() {
    // 3 of 5 ground-truth ids appear in actual top-5.
    let actual = vec![1, 2, 3, 99, 100];
    let truth = vec![1, 2, 3, 4, 5];
    assert!((recall_at_k(&actual, &truth, 5) - 0.6).abs() < 1e-9);
}

#[test]
fn recall_order_insensitive() {
    // Recall doesn't reward correct ordering — that's NDCG's job.
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
fn recall_truth_shorter_than_k_uses_truth_len() {
    // If ground truth has fewer than k entries, divide by truth_k, not k.
    let actual = vec![1, 2];
    let truth = vec![1, 2];
    assert_eq!(recall_at_k(&actual, &truth, 10), 1.0);
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
fn ndcg_rewards_earlier_matches() {
    // Both lists have the same set of relevant ids, but in different orders.
    // The list with relevant ids at lower (better) ranks should have a
    // higher NDCG.
    let truth = vec![1, 2, 3];
    let early = vec![1, 2, 3, 99, 100]; // relevant at ranks 0, 1, 2
    let late = vec![99, 100, 1, 2, 3]; // relevant at ranks 2, 3, 4

    let early_ndcg = ndcg_at_k(&early, &truth, 5);
    let late_ndcg = ndcg_at_k(&late, &truth, 5);

    assert!(
        early_ndcg > late_ndcg,
        "{early_ndcg} should beat {late_ndcg}"
    );
    assert!(early_ndcg <= 1.0);
    assert!(late_ndcg >= 0.0);
}

#[test]
fn ndcg_k_zero_is_zero() {
    let actual = vec![1, 2, 3];
    let truth = vec![1, 2, 3];
    assert_eq!(ndcg_at_k(&actual, &truth, 0), 0.0);
}

#[test]
fn ndcg_value_in_unit_interval() {
    // Random-ish overlap. Just assert the [0, 1] invariant holds.
    let actual = vec![3, 1, 99, 2, 100];
    let truth = vec![1, 2, 3, 4, 5];
    let n = ndcg_at_k(&actual, &truth, 5);
    assert!((0.0..=1.0).contains(&n), "ndcg out of range: {n}");
}
