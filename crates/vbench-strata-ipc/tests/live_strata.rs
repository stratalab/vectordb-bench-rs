//! End-to-end test against a real strata daemon.
//!
//! These tests are `#[ignore]` by default because they require:
//! 1. The `STRATA_BIN` environment variable pointing at a `strata` binary
//!    (typically `~/.strata/bin/strata` after running install.sh).
//! 2. The ability to spawn that binary as a child process.
//!
//! Run them locally with:
//!
//! ```bash
//! STRATA_BIN=~/.strata/bin/strata cargo test -p vbench-strata-ipc -- --ignored
//! ```
//!
//! CI runs the cheap pure-serde tests in `round_trip.rs`. The release
//! pipeline (when added) should also exercise this file against the latest
//! strata release as a smoke test for wire-format drift.

use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tempfile::TempDir;
use tokio::process::Command as TokioCommand;
use tokio::time::{sleep, Instant};

use vbench_strata_ipc::{BatchVectorEntry, Command, DistanceMetric, Output, StrataIpcClient};

const COLLECTION: &str = "vbench_smoke";
const DIM: u64 = 8;

async fn wait_for_socket(path: &std::path::Path, timeout: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("strata.sock did not appear within {timeout:?}"),
            ));
        }
        sleep(Duration::from_millis(50)).await;
    }
}

fn locate_strata_bin() -> Option<PathBuf> {
    env::var_os("STRATA_BIN").map(PathBuf::from)
}

#[tokio::test]
#[ignore = "requires STRATA_BIN env var pointing at a strata binary"]
async fn ping_round_trip_against_real_daemon() {
    let strata_bin = match locate_strata_bin() {
        Some(p) => p,
        None => {
            eprintln!("STRATA_BIN not set; skipping");
            return;
        }
    };

    let workdir = TempDir::new().expect("tempdir");
    let mut child = TokioCommand::new(&strata_bin)
        .arg("up")
        .arg("--fg")
        .arg("--db")
        .arg(workdir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn strata");

    let socket = workdir.path().join("strata.sock");
    wait_for_socket(&socket, Duration::from_secs(10))
        .await
        .expect("socket appears");

    let mut client = StrataIpcClient::connect(&socket).await.expect("connect");
    let version = client.ping().await.expect("ping");
    assert!(!version.is_empty(), "server returned empty version");
    eprintln!("strata version: {version}");

    // Cleanly shut the daemon down so the tempdir can be removed.
    let _ = child.kill().await;
}

#[tokio::test]
#[ignore = "requires STRATA_BIN env var pointing at a strata binary"]
async fn vector_create_upsert_query_against_real_daemon() {
    let strata_bin = match locate_strata_bin() {
        Some(p) => p,
        None => {
            eprintln!("STRATA_BIN not set; skipping");
            return;
        }
    };

    let workdir = TempDir::new().expect("tempdir");
    let mut child = TokioCommand::new(&strata_bin)
        .arg("up")
        .arg("--fg")
        .arg("--db")
        .arg(workdir.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn strata");

    let socket = workdir.path().join("strata.sock");
    wait_for_socket(&socket, Duration::from_secs(10))
        .await
        .expect("socket appears");

    let mut client = StrataIpcClient::connect(&socket).await.expect("connect");

    // Create the collection.
    let create_out = client
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: COLLECTION.to_string(),
            dimension: DIM,
            metric: DistanceMetric::Cosine,
        })
        .await
        .expect("create");
    assert!(matches!(create_out, Output::Version(_)));

    // Insert 10 distinct unit vectors.
    let entries: Vec<BatchVectorEntry> = (0..10u64)
        .map(|i| {
            let mut v = vec![0.0; DIM as usize];
            v[(i as usize) % DIM as usize] = 1.0;
            BatchVectorEntry {
                key: i.to_string(),
                vector: v,
                metadata: None,
            }
        })
        .collect();
    let upsert_out = client
        .execute(Command::VectorBatchUpsert {
            branch: None,
            space: None,
            collection: COLLECTION.to_string(),
            entries,
        })
        .await
        .expect("upsert");
    match upsert_out {
        Output::Versions(vs) => assert_eq!(vs.len(), 10),
        other => panic!("expected Versions, got {other:?}"),
    }

    // Query for nearest neighbours of vector 3 — should return key "3" first.
    let mut query = vec![0.0; DIM as usize];
    query[3] = 1.0;
    let query_out = client
        .execute(Command::VectorQuery {
            branch: None,
            space: None,
            collection: COLLECTION.to_string(),
            query,
            k: 3,
            filter: None,
            metric: None,
            as_of: None,
        })
        .await
        .expect("query");
    match query_out {
        Output::VectorMatches(matches) => {
            assert!(!matches.is_empty(), "no matches returned");
            assert_eq!(matches[0].key, "3", "expected key '3' as nearest");
        }
        other => panic!("expected VectorMatches, got {other:?}"),
    }

    // Cleanup.
    let _ = client
        .execute(Command::VectorDeleteCollection {
            branch: None,
            space: None,
            collection: COLLECTION.to_string(),
        })
        .await;

    let _ = child.kill().await;
}
