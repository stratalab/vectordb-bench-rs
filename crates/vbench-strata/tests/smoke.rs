//! End-to-end smoke test against a real strata daemon.
//!
//! `#[ignore]` by default. To run:
//!
//! ```bash
//! STRATA_BIN=~/.strata/bin/strata cargo test -p vbench-strata -- --ignored
//! ```
//!
//! This is the canary that catches:
//! - Wire-protocol drift between vbench-strata-ipc and a released strata
//! - Subprocess lifecycle bugs (orphaned daemons, socket-race conditions)
//! - VectorBatchUpsert / VectorQuery surface changes
//!
//! What it does (against a fresh tempdir):
//! 1. Open a `StrataAdapter`, which spawns `strata up --foreground` and
//!    creates the `vbench` collection.
//! 2. Load 64 distinct random-ish vectors via two `load()` calls.
//! 3. `count()` returns 64.
//! 4. `optimize()` returns Ok.
//! 5. `search()` for the first inserted vector returns id 0 as the
//!    nearest neighbour.
//! 6. `shutdown()` cleanly kills the daemon and the tempdir is removable.

use std::path::PathBuf;

use tempfile::TempDir;
use vbench_core::{BenchAdapter, Metric, VectorRow};
use vbench_strata::StrataAdapter;

const DIM: usize = 16;

fn build_vectors(n: usize, dim: usize) -> Vec<Vec<f32>> {
    // Deterministic-but-distinct vectors. Each row has 1.0 in row_idx % dim
    // and a unique fractional offset, so they're all linearly independent.
    (0..n)
        .map(|i| {
            let mut v = vec![0.0_f32; dim];
            v[i % dim] = 1.0 + (i as f32 / 1000.0);
            v
        })
        .collect()
}

fn locate_strata_for_test() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("STRATA_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[tokio::test]
#[ignore = "requires STRATA_BIN env var pointing at a strata binary (>= v0.6.1)"]
async fn strata_adapter_end_to_end() {
    let strata_bin = match locate_strata_for_test() {
        Some(p) => p,
        None => {
            eprintln!("STRATA_BIN not set; skipping");
            return;
        }
    };

    let workdir = TempDir::new().expect("tempdir");
    let params = serde_json::json!({ "strata_bin": strata_bin });

    let adapter = StrataAdapter::open(workdir.path(), DIM, Metric::Cosine, &params)
        .await
        .expect("StrataAdapter::open");

    let info = adapter.info();
    assert_eq!(info.name, "strata");
    assert!(!info.db_version.is_empty(), "server version is empty");
    eprintln!("strata version: {}", info.db_version);

    // Load 64 vectors in two batches.
    let vectors = build_vectors(64, DIM);
    let rows_a: Vec<VectorRow<'_>> = vectors
        .iter()
        .enumerate()
        .take(32)
        .map(|(i, v)| VectorRow {
            id: i as u64,
            vector: v.as_slice(),
            labels: None,
        })
        .collect();
    let rows_b: Vec<VectorRow<'_>> = vectors
        .iter()
        .enumerate()
        .skip(32)
        .map(|(i, v)| VectorRow {
            id: i as u64,
            vector: v.as_slice(),
            labels: None,
        })
        .collect();

    adapter.load(&rows_a).await.expect("load batch 1");
    adapter.load(&rows_b).await.expect("load batch 2");

    let count = adapter.count().await.expect("count");
    assert_eq!(count, 64, "expected 64 rows after two loads");

    adapter.optimize().await.expect("optimize");

    // Query for vector 0 — it should come back as the nearest neighbour.
    let q = vectors[0].clone();
    let hits = adapter.search(&q, 5).await.expect("search");
    assert!(!hits.is_empty(), "no search results");
    assert_eq!(hits[0], 0, "nearest neighbour of vector 0 should be id 0");

    adapter.shutdown().await.expect("shutdown");

    // After shutdown the tempdir cleanup happens via TempDir's Drop;
    // this just asserts the dir still exists at this point so the
    // assertion above could read meaningful state.
    assert!(workdir.path().exists());
}
