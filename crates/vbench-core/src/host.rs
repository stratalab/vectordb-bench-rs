//! Host snapshot for result provenance.
//!
//! Captured once at the start of every run and embedded in the `TestResult`'s
//! `timestamps.host` field. Used so reviewers can tell which machine
//! produced any given number, without having to ask.

use serde::{Deserialize, Serialize};

/// Snapshot of the host the benchmark ran on.
///
/// Intentionally narrow: just enough for a reader to recognise the box.
/// Anything that varies run-to-run (CPU temperature, current load) is
/// deliberately excluded — those belong in a separate observability layer,
/// not in the result document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    /// `hostname(1)` output.
    pub hostname: String,
    /// `std::env::consts::OS` (e.g. "linux", "macos").
    pub os: String,
    /// `std::env::consts::ARCH` (e.g. "x86_64", "aarch64").
    pub arch: String,
    /// CPU brand string from sysinfo (e.g. "AMD Ryzen 9 7950X").
    pub cpu_brand: String,
    /// Logical core count (includes SMT threads).
    pub cpu_cores: usize,
    /// Total physical RAM in bytes.
    pub total_memory_bytes: u64,
    /// Rust compiler version that built the binary, e.g. "rustc 1.94.1".
    /// Captured at run-time via the `RUSTC_VERSION` env var if present, else
    /// `None` — vbench-cli sets it via a `build.rs` so end users see a
    /// concrete value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rustc_version: Option<String>,
}

impl HostInfo {
    /// Snapshot the current host.
    ///
    /// Best-effort: missing values default to `"unknown"` rather than
    /// panicking. Cheap enough to call once per run.
    pub fn snapshot() -> Self {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();

        let cpu_brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Self {
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_brand,
            cpu_cores: num_cpus_logical(),
            total_memory_bytes: sys.total_memory(),
            rustc_version: option_env!("RUSTC_VERSION").map(str::to_string),
        }
    }
}

fn num_cpus_logical() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
}
