//! Async unix-socket client for the strata IPC daemon.
//!
//! Single connection, single in-flight request at a time. The strata server
//! supports multiple concurrent clients (up to 128) but pipelining within a
//! single connection isn't part of the wire protocol — every response is
//! tagged with the request id and the server replies in request order.

use std::path::Path;

use tokio::io::BufStream;
use tokio::net::UnixStream;

use crate::error::{IpcClientError, Result};
use crate::types::{Command, Output, Request, Response};
use crate::wire::{read_frame, write_frame};

/// Async client for the strata IPC daemon (`strata up`).
///
/// Holds a single buffered unix-socket connection. Not `Clone` — open one
/// `StrataIpcClient` per concurrent benchmark thread.
pub struct StrataIpcClient {
    stream: BufStream<UnixStream>,
    next_id: u64,
}

impl StrataIpcClient {
    /// Connect to the strata daemon at the given socket path.
    ///
    /// The standard socket location for `strata up <db_path>` is
    /// `<db_path>/strata.sock`. The caller is responsible for ensuring the
    /// daemon is running and the socket exists before calling `connect`
    /// (e.g. by polling for the file in a startup loop).
    pub async fn connect(socket: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket).await?;
        Ok(Self {
            stream: BufStream::new(stream),
            next_id: 1,
        })
    }

    /// Issue a command and await its response.
    ///
    /// Returns the `Output` on success, or an `IpcClientError::ServerError`
    /// wrapping the opaque server-side error on failure.
    pub async fn execute(&mut self, command: Command) -> Result<Output> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let request = Request { id, command };
        let payload = rmp_serde::to_vec_named(&request)?;
        write_frame(&mut self.stream, &payload).await?;

        let bytes = read_frame(&mut self.stream).await?;
        let response: Response = rmp_serde::from_slice(&bytes)?;

        if response.id != id {
            return Err(IpcClientError::ResponseIdMismatch {
                expected: id,
                got: response.id,
            });
        }

        match response.result {
            Ok(output) => Ok(output),
            Err(server_err) => Err(IpcClientError::ServerError(server_err)),
        }
    }

    /// Send a `Ping` and return the server's reported version string.
    ///
    /// Used at connect-time as a handshake and to populate
    /// `db_config.strata_version` in the published result JSON.
    pub async fn ping(&mut self) -> Result<String> {
        match self.execute(Command::Ping).await? {
            Output::Pong { version } => Ok(version),
            other => Err(IpcClientError::UnexpectedOutput(format!("{other:?}"))),
        }
    }
}
