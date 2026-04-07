//! Core types and runner for vectordb-bench-rs.
//!
//! This crate has no DB-specific dependencies. Each adapter (`vbench-strata`,
//! `vbench-qdrant`, …) implements the [`BenchAdapter`] trait against the
//! vector DB it wraps, and the runner here drives every adapter through the
//! same phase sequence:
//!
//! 1. **load** — stream a dataset's training vectors into the DB
//! 2. **optimize** — adapter-specific warm-up (HNSW lazy build, index merge, etc.)
//! 3. **recall** — serial loop over the dataset's test queries, comparing
//!    top-k results against ground truth
//! 4. **serial latency** — folded into the recall loop in Phase 1
//! 5. **concurrent QPS** — Phase 2; not implemented yet
//!
//! The output is a [`TestResult`] JSON document whose field names match
//! VectorDBBench's `vectordb_bench/models.py:TestResult` schema, so reviewers
//! can drop our numbers into the existing leaderboard tooling.

#![warn(missing_docs)]

pub mod adapter;
pub mod error;
pub mod host;
pub mod metrics;
pub mod result;

pub use adapter::{AdapterInfo, BenchAdapter, Metric, VectorRow};
pub use error::{Result, VbenchError};
pub use host::HostInfo;
pub use metrics::{ndcg_at_k, recall_at_k, LatencyHistogram};
pub use result::{CaseConfig, DbConfig, ResultMetrics, TaskConfig, TestResult, Timestamps};
