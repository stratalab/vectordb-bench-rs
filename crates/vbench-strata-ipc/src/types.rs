//! Wire types mirroring the strata-core IPC protocol.
//!
//! These structs are field-for-field equivalents of the matching types in
//! `strata-core` (verified at commit `ff71312c`):
//!
//! - `Command` variants — `crates/executor/src/command.rs`
//! - `Output` variants  — `crates/executor/src/output.rs`
//! - `BatchVectorEntry`, `VectorMatch`, `DistanceMetric` — `crates/executor/src/types.rs`
//!
//! ## Why field names matter
//!
//! `strata-core` serialises with `rmp_serde::to_vec_named`, which encodes
//! struct fields by **name** (MessagePack map with string keys), not by
//! positional index. Renaming a field on either side breaks the wire format.
//!
//! ## Opaque fields
//!
//! Two types of fields use `Option<rmpv::Value>` instead of mirroring the
//! upstream concrete type:
//!
//! - `metadata` (on `BatchVectorEntry` and `VectorMatch`) — strata-core uses
//!   its own `Value` enum here, which is large and irrelevant to a vector
//!   benchmark. We always send `None` and accept whatever the server returns.
//!
//! - `IpcError` — strata-core's `Error` enum has 50+ externally-tagged
//!   variants. Mirroring all of them is impractical. We capture the whole
//!   error as an opaque `rmpv::Value` and pretty-print it on Display.

use serde::{Deserialize, Serialize};

// =============================================================================
// Request / Response framing
// =============================================================================

/// IPC request from client to server.
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    /// Monotonically increasing request id, echoed in the matching response.
    pub id: u64,
    /// The command to execute.
    pub command: Command,
}

/// IPC response from server to client.
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    /// Matches the originating request id.
    pub id: u64,
    /// Result of executing the command. `Ok(Output)` on success, `Err(...)`
    /// captured opaquely on failure.
    pub result: Result<Output, IpcError>,
}

// =============================================================================
// Command — only the variants vbench actually issues
// =============================================================================

/// Subset of `strata_executor::Command` that vbench needs.
///
/// **Important:** the variant order does not matter (serde uses externally
/// tagged enums by default), but the variant *names* and field *names* must
/// match the strata-core definitions exactly.
#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    /// Health probe / version handshake. Returns `Output::Pong { version }`.
    Ping,

    /// Create a vector collection.
    VectorCreateCollection {
        /// Optional target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Optional target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vector dimensionality.
        dimension: u64,
        /// Distance metric.
        metric: DistanceMetric,
    },

    /// Bulk insert / update of vector entries in a single transaction.
    VectorBatchUpsert {
        /// Optional target branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Optional target space.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vectors to upsert.
        entries: Vec<BatchVectorEntry>,
    },

    /// k-nearest-neighbour search.
    VectorQuery {
        /// Optional target branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Optional target space.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Query embedding.
        query: Vec<f32>,
        /// Number of neighbours to return.
        k: u64,
        /// Optional metadata filters. vbench never uses this; we always send
        /// `None`. Declared as `Option<rmpv::Value>` so the server's filter
        /// shape doesn't leak into our type surface.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<rmpv::Value>,
        /// Optional distance-metric override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metric: Option<DistanceMetric>,
        /// Optional time-travel timestamp (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Delete a collection (cleanup).
    VectorDeleteCollection {
        /// Optional target branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Optional target space.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
    },

    /// Get statistics for a single collection (vector count etc.).
    VectorCollectionStats {
        /// Optional target branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Optional target space.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
    },
}

// =============================================================================
// Output — only the variants vbench needs to read
// =============================================================================

/// Subset of `strata_executor::Output` that vbench reads.
///
/// Variants we don't list are not deserialised. If a future strata version
/// returns an Output variant we don't recognise, deserialisation will error
/// with "unknown variant" — that's a desired failure mode (loud breakage on
/// drift, not silent corruption).
#[derive(Debug, Serialize, Deserialize)]
pub enum Output {
    /// Response to `Command::Ping`. The version is the strata daemon's
    /// `--version` string.
    Pong {
        /// Server version (e.g. "0.6.1").
        version: String,
    },
    /// Single commit version (returned by `VectorCreateCollection` etc.).
    Version(u64),
    /// Vector of commit versions (returned by `VectorBatchUpsert`, one per
    /// upserted entry — for batched commits all entries share the same
    /// version).
    Versions(Vec<u64>),
    /// k-NN search results.
    VectorMatches(Vec<VectorMatch>),
    /// Boolean (returned by `VectorDeleteCollection`).
    Bool(bool),
    /// Collection list (returned by `VectorCollectionStats` as a single-entry
    /// list). Captured opaquely as `rmpv::Value` because the inner
    /// `CollectionInfo` shape carries fields vbench doesn't need.
    VectorCollectionList(Vec<rmpv::Value>),
}

// =============================================================================
// Shared types
// =============================================================================

/// Distance metric for vector similarity.
///
/// Mirrors `strata_executor::types::DistanceMetric`. Snake-case rename matches
/// the upstream `#[serde(rename_all = "snake_case")]` attribute, so a value
/// like `DistanceMetric::Cosine` serialises as the MessagePack string
/// `"cosine"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    /// Cosine similarity (default).
    #[default]
    Cosine,
    /// Euclidean (L2) distance.
    Euclidean,
    /// Dot product similarity.
    DotProduct,
}

/// Batch vector entry for `VectorBatchUpsert`.
///
/// Mirrors `strata_executor::types::BatchVectorEntry`. The `metadata` field is
/// declared as `Option<rmpv::Value>` (rather than mirroring strata-core's
/// `Value` enum) because vbench never sends or interprets metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchVectorEntry {
    /// Vector key. vbench stringifies u64 row ids.
    pub key: String,
    /// The embedding.
    pub vector: Vec<f32>,
    /// Optional metadata (always `None` for vbench).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<rmpv::Value>,
}

/// Single match returned by `VectorQuery`.
///
/// Mirrors `strata_executor::types::VectorMatch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorMatch {
    /// Key of the matched vector.
    pub key: String,
    /// Similarity score (higher = more similar).
    pub score: f32,
    /// Optional metadata, captured opaquely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<rmpv::Value>,
}

// =============================================================================
// IpcError — opaque capture of strata-core's Error enum
// =============================================================================

/// Opaque wrapper for `strata_executor::Error`.
///
/// strata-core's `Error` enum has 50+ externally-tagged variants. Mirroring
/// all of them in vbench is impractical and brittle. Instead we deserialise
/// the entire error as an `rmpv::Value` and provide a `Display` impl that
/// pretty-prints it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IpcError(pub rmpv::Value);

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // strata-core errors serialise as `{"VariantName": {field: value, ...}}`.
        // Walk one level so error messages are readable.
        if let Some(map) = self.0.as_map() {
            if let Some((tag, body)) = map.first() {
                if let Some(name) = tag.as_str() {
                    return write!(f, "{name}({body})");
                }
            }
        }
        write!(f, "{:?}", self.0)
    }
}

impl std::error::Error for IpcError {}
