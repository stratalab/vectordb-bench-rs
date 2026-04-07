//! Hand-rolled MessagePack IPC client for the strata daemon.
//!
//! `vectordb-bench-rs` deliberately depends on the precompiled `strata` binary
//! delivered via `https://stratadb.org/install.sh`, not on `strata-executor`
//! source. To talk to that binary we drive its built-in IPC server (started
//! with `strata up`) over its unix-socket / length-prefixed-MessagePack
//! protocol.
//!
//! This crate mirrors **only the small subset** of the wire protocol vbench
//! actually needs:
//!
//! - `Command::Ping` — handshake / version probe
//! - `Command::VectorCreateCollection` — provision the bench collection
//! - `Command::VectorBatchUpsert` — bulk-load vectors
//! - `Command::VectorQuery` — k-NN search
//! - `Command::VectorDeleteCollection` — cleanup
//! - `Command::VectorCollectionStats` — sanity-check counts
//!
//! The wire format is verified against `strata-core` at `ff71312c`:
//!
//! - `crates/executor/src/ipc/wire.rs` — 4-byte BE length prefix + payload, 64 MB cap
//! - `crates/executor/src/ipc/protocol.rs` — `Request { id, command }` /
//!   `Response { id, result }`, encoded with `rmp_serde::to_vec_named`
//! - `crates/executor/src/command.rs` — `Command` variant field names
//! - `crates/executor/src/output.rs` — `Output` variant field names
//! - `crates/executor/src/types.rs` — `BatchVectorEntry`, `VectorMatch`, `DistanceMetric`
//!
//! Stability strategy: at connect time the client sends a `Ping` and records
//! the server version. The result JSON of every benchmark run includes that
//! version under `db_config.strata_version` so any wire-format drift can be
//! attributed to a specific strata release.

#![warn(missing_docs)]

mod client;
mod error;
mod types;
mod wire;

pub use client::StrataIpcClient;
pub use error::{IpcClientError, Result};
pub use types::{
    BatchVectorEntry, Command, DistanceMetric, IpcError, Output, Request, Response, VectorMatch,
};
