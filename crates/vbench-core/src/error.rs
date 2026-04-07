//! Error type for vbench-core.

use thiserror::Error;

/// Result alias used throughout vbench-core.
pub type Result<T> = std::result::Result<T, VbenchError>;

/// Errors raised by vbench-core's runner, dataset loader, and metrics modules.
///
/// Adapter-side errors are wrapped via [`VbenchError::Adapter`] so a runner
/// failure can carry the underlying DB error string without `vbench-core`
/// needing to know which adapter produced it.
#[derive(Debug, Error)]
pub enum VbenchError {
    /// I/O error reading or writing a dataset, cache file, or output JSON.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialise / deserialise error (typically when emitting the
    /// final `TestResult`).
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// hdrhistogram out-of-range value (would only happen for sub-microsecond
    /// or > 60-second latency samples, neither of which are realistic).
    #[error("histogram error: {0}")]
    Histogram(String),

    /// Configuration / input-validation error (e.g. unknown dataset id,
    /// recall_k > dataset's neighbour count).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Wrapper for an adapter-side error. The string is the adapter's own
    /// error formatted via `Display`.
    #[error("adapter error: {0}")]
    Adapter(String),
}

impl From<hdrhistogram::errors::RecordError> for VbenchError {
    fn from(e: hdrhistogram::errors::RecordError) -> Self {
        VbenchError::Histogram(e.to_string())
    }
}

impl From<hdrhistogram::errors::CreationError> for VbenchError {
    fn from(e: hdrhistogram::errors::CreationError) -> Self {
        VbenchError::Histogram(e.to_string())
    }
}
