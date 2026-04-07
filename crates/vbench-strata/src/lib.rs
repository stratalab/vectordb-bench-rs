//! [`BenchAdapter`] implementation for the Strata embedded vector database.
//!
//! ## How vbench drives Strata
//!
//! vbench-strata never imports `strata-executor` source. Instead it consumes
//! the precompiled `strata` binary delivered via
//! `https://stratadb.org/install.sh`. At [`StrataAdapter::open`] time the
//! adapter:
//!
//! 1. Locates the `strata` binary (`STRATA_BIN` env, then `PATH`, then
//!    `~/.strata/bin/strata`).
//! 2. Spawns `strata up --foreground <workdir>` as a tokio child process
//!    with `kill_on_drop`.
//! 3. Polls for the unix socket at `<workdir>/strata.sock` (10 s timeout).
//! 4. Connects an [`vbench_strata_ipc::StrataIpcClient`] to the socket.
//! 5. Issues `Ping` to record the server version (lands in
//!    `db_config.version` of the published result).
//! 6. Issues `VectorCreateCollection` to provision the bench collection.
//!
//! `load`, `optimize`, and `search` then push commands over the IPC client.
//! `optimize` issues a single warm-up query (zero vector) to force Strata's
//! lazy HNSW build before the recall phase starts measuring.
//!
//! `shutdown` drops the client (closing the socket) and kills the child.
//!
//! ## Adapter parameters
//!
//! The opaque `params: serde_json::Value` argument to
//! [`vbench_core::BenchAdapter::open`] supports a single optional field:
//!
//! ```json
//! { "strata_bin": "/path/to/strata" }
//! ```
//!
//! When set, this overrides the binary lookup. Useful for benchmarking a
//! locally-built strata against the released one.

#![warn(missing_docs)]

mod adapter;
mod locate;

pub use adapter::StrataAdapter;
pub use locate::{find_strata_bin, LocateError};
