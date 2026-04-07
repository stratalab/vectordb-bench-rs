//! Error types for the IPC client.

use thiserror::Error;

use crate::types::IpcError;

/// Result alias for client operations.
pub type Result<T> = std::result::Result<T, IpcClientError>;

/// Errors from the IPC client.
#[derive(Debug, Error)]
pub enum IpcClientError {
    /// I/O error reading or writing the unix socket.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// MessagePack encode failure (request).
    #[error("encode error: {0}")]
    Encode(#[from] rmp_serde::encode::Error),

    /// MessagePack decode failure (response).
    ///
    /// Most commonly raised when the server returns an `Output` variant we
    /// don't mirror in this crate — for example, after a strata upgrade
    /// introduces a new variant. The variant name is included in the
    /// underlying serde error and is the actionable signal.
    #[error("decode error: {0}")]
    Decode(#[from] rmp_serde::decode::Error),

    /// A frame exceeded the 64 MB cap on either the encode or decode path.
    #[error("frame too large: {bytes} bytes (max {max})")]
    FrameTooLarge {
        /// Frame size that triggered the error.
        bytes: usize,
        /// The configured max.
        max: usize,
    },

    /// Server returned a structured error (`Result::Err` in `Response`).
    #[error("server error: {0}")]
    ServerError(IpcError),

    /// Response id did not match the request id (would only happen if the
    /// server pipelined or reordered, which it does not).
    #[error("response id mismatch: expected {expected}, got {got}")]
    ResponseIdMismatch {
        /// The id we sent.
        expected: u64,
        /// The id the server returned.
        got: u64,
    },

    /// `execute()` called with an `Output` variant the caller didn't expect.
    /// Lets adapter code reject e.g. `Output::Bool(false)` when it asked for
    /// `Output::Versions`.
    #[error("unexpected output variant: {0}")]
    UnexpectedOutput(String),
}
