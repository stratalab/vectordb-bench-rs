//! [`vbench_core::BenchAdapter`] implementation that drives the released
//! `strata` binary over its IPC daemon.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{info, warn};

use vbench_core::{AdapterInfo, BenchAdapter, Metric, VectorRow};
use vbench_strata_ipc::{BatchVectorEntry, Command, DistanceMetric, Output, StrataIpcClient};

use crate::locate::find_strata_bin;

const COLLECTION_NAME: &str = "vbench";
const SOCKET_FILENAME: &str = "strata.sock";
const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(50);
const SOCKET_POLL_TIMEOUT: Duration = Duration::from_secs(10);

/// Strata adapter for vectordb-bench-rs.
///
/// Drives the released `strata` binary as a child process via its IPC
/// daemon. Single-connection: all `BenchAdapter` calls go through one
/// `StrataIpcClient` wrapped in a `tokio::sync::Mutex`. Phase 2 will need
/// to think harder about pooling for the concurrent QPS phase.
pub struct StrataAdapter {
    /// Child process running `strata up --foreground <workdir>`.
    /// `kill_on_drop` is set so a panic in the runner doesn't leak the
    /// daemon. Held in an `Option` so [`StrataAdapter::shutdown`] can
    /// `take()` and explicitly `kill().await` it.
    child: Option<Child>,
    /// Single IPC client. `BenchAdapter` methods take `&self`, but
    /// `StrataIpcClient::execute` takes `&mut self`, so we wrap in a
    /// `tokio::sync::Mutex`.
    client: Mutex<StrataIpcClient>,
    /// `strata --version` reported via `Ping` at open time. Lands in the
    /// published result's `db_config.version`.
    server_version: String,
    /// Collection name (constant for now: "vbench").
    collection: String,
    /// Adapter-side row counter, incremented by [`StrataAdapter::load`].
    /// Returned from [`StrataAdapter::count`] for the runner's post-load
    /// sanity check.
    loaded_count: AtomicU64,
}

/// Adapter parameters parsed from the opaque
/// `vbench_core::BenchAdapter::open` JSON value.
#[derive(Debug, Default, serde::Deserialize)]
struct Params {
    /// Optional explicit path to the `strata` binary. If absent, the
    /// adapter falls back to the [`find_strata_bin`] lookup chain
    /// (`STRATA_BIN` env, `PATH`, `~/.strata/bin/strata`).
    #[serde(default)]
    strata_bin: Option<PathBuf>,
}

#[async_trait]
impl BenchAdapter for StrataAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            name: "strata".to_string(),
            db_version: self.server_version.clone(),
            notes: Some("embedded; driven via `strata up` IPC daemon over unix socket".to_string()),
        }
    }

    async fn open(
        workdir: &Path,
        dim: usize,
        metric: Metric,
        params: &serde_json::Value,
    ) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let parsed: Params = serde_json::from_value(params.clone()).unwrap_or_default();

        let strata_bin = find_strata_bin(parsed.strata_bin.as_deref())?;
        info!(strata_bin = ?strata_bin, "located strata binary");

        // Strata expects an empty workdir on first start; the runner is
        // responsible for handing us a fresh tempdir, so we don't need to
        // wipe anything ourselves.
        std::fs::create_dir_all(workdir)?;

        // Strata >=0.6.1 takes the workdir via `--db <PATH>` and uses
        // `--fg` (not `--foreground`) for the foreground flag.
        let mut child = TokioCommand::new(&strata_bin)
            .arg("up")
            .arg("--fg")
            .arg("--db")
            .arg(workdir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let socket = workdir.join(SOCKET_FILENAME);
        match wait_for_socket(&socket, SOCKET_POLL_TIMEOUT).await {
            Ok(()) => {}
            Err(e) => {
                // Daemon failed to come up — kill it and surface the error.
                let _ = child.kill().await;
                return Err(e.into());
            }
        }

        let mut client = StrataIpcClient::connect(&socket).await?;
        let server_version = client.ping().await?;
        info!(version = %server_version, "connected to strata daemon");

        // Provision the bench collection.
        let create_out = client
            .execute(Command::VectorCreateCollection {
                branch: None,
                space: None,
                collection: COLLECTION_NAME.to_string(),
                dimension: dim as u64,
                metric: map_metric(metric),
            })
            .await?;
        match create_out {
            Output::Version(_) => {}
            other => anyhow::bail!("unexpected output from VectorCreateCollection: {other:?}"),
        }

        Ok(Self {
            child: Some(child),
            client: Mutex::new(client),
            server_version,
            collection: COLLECTION_NAME.to_string(),
            loaded_count: AtomicU64::new(0),
        })
    }

    async fn load(&self, rows: &[VectorRow<'_>]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        // Build BatchVectorEntry vec. Stringifying the u64 id is the
        // documented allocation tax — Strata's IPC `key` is `String`. We
        // could swap to `itoa::Buffer` if profiling shows this hot.
        let entries: Vec<BatchVectorEntry> = rows
            .iter()
            .map(|r| BatchVectorEntry {
                key: r.id.to_string(),
                vector: r.vector.to_vec(),
                metadata: None,
            })
            .collect();
        let n = entries.len() as u64;

        let mut client = self.client.lock().await;
        let out = client
            .execute(Command::VectorBatchUpsert {
                branch: None,
                space: None,
                collection: self.collection.clone(),
                entries,
            })
            .await?;
        match out {
            Output::Versions(v) => {
                if v.len() as u64 != n {
                    anyhow::bail!(
                        "VectorBatchUpsert returned {} versions for {} rows",
                        v.len(),
                        n
                    );
                }
            }
            other => anyhow::bail!("unexpected output from VectorBatchUpsert: {other:?}"),
        }
        self.loaded_count.fetch_add(n, Ordering::SeqCst);
        Ok(())
    }

    async fn optimize(&self) -> anyhow::Result<()> {
        // No-op for Strata. Strata's HNSW build is lazy: the first
        // VectorQuery after a load triggers the in-memory index
        // construction. The runner immediately issues `warmup_queries`
        // real test queries after this call, and those force the build
        // exactly as well as anything we could do here. The `optimize`
        // wall-clock the runner measures includes that warmup loop, so
        // the total `optimize_duration` correctly accounts for the
        // first-query cold path.
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<u64>> {
        let mut client = self.client.lock().await;
        let out = client
            .execute(Command::VectorQuery {
                branch: None,
                space: None,
                collection: self.collection.clone(),
                query: query.to_vec(),
                k: k as u64,
                filter: None,
                metric: None,
                as_of: None,
            })
            .await?;
        match out {
            Output::VectorMatches(matches) => {
                let ids: Vec<u64> = matches
                    .into_iter()
                    .map(|m| m.key.parse::<u64>().unwrap_or(u64::MAX))
                    .collect();
                Ok(ids)
            }
            other => anyhow::bail!("unexpected output from VectorQuery: {other:?}"),
        }
    }

    async fn count(&self) -> anyhow::Result<u64> {
        // Phase 1 trusts our adapter-side counter — every successful
        // VectorBatchUpsert increments it, and the runner uses count() only
        // as a "did the load finish" sanity check. Phase 2 may want to
        // round-trip via VectorCollectionStats for stronger validation.
        Ok(self.loaded_count.load(Ordering::SeqCst))
    }

    async fn shutdown(mut self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        // Best-effort cleanup: drop the IPC client first (closes the
        // socket), then kill the child. Errors are warned but not
        // propagated — partial shutdown is still better than leaking the
        // daemon.
        drop(self.client);

        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.kill().await {
                warn!(error = %e, "failed to kill strata child process");
            }
        }
        Ok(())
    }
}

fn map_metric(m: Metric) -> DistanceMetric {
    match m {
        Metric::Cosine => DistanceMetric::Cosine,
        Metric::L2 => DistanceMetric::Euclidean,
        Metric::Ip => DistanceMetric::DotProduct,
    }
}

/// Poll for the existence of `socket` until it appears or `timeout` elapses.
async fn wait_for_socket(socket: &Path, timeout: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if socket.exists() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("strata.sock did not appear within {timeout:?}"),
            ));
        }
        sleep(SOCKET_POLL_INTERVAL).await;
    }
}
