//! Parquet decoders for the VectorDBBench-hosted dataset bundles.
//!
//! Two decoders, one per file shape:
//!
//! - [`read_embeddings_parquet`] decodes a `(id: int64, emb: List<Float32>)`
//!   parquet (the train.parquet / test.parquet shape) into a flat
//!   `Vec<f32>` of length `num_rows * dim`. Returns `(num_rows, flat)`.
//!
//! - [`read_neighbours_parquet`] decodes a
//!   `(id: int64, neighbors_id: List<Int64>)` parquet (neighbors.parquet)
//!   into a `Vec<Vec<u64>>` (one entry per query).
//!
//! Both decoders use the synchronous `parquet::arrow::arrow_reader` API.
//! Async streaming would let us start ingesting before the file is fully
//! downloaded, but for Phase 1 the simpler sync path is enough — the
//! downloader writes the whole file before we touch it, and parquet decode
//! is fast relative to network IO at multi-GB scale.
//!
//! ## Embedding column shape
//!
//! VectorDBBench's hosted bundles use Arrow `List<Float32>` (variable-length
//! list backed by an offsets buffer) even though every row has the same
//! length. We handle the common case (`ListArray`) and the alternative
//! (`FixedSizeListArray`) so the decoder is robust to either layout.

use std::fs::File;
use std::path::Path;

use arrow::array::{Array, FixedSizeListArray, Int64Array, ListArray};
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::error::{Result, VbenchError};

/// Decode an `(id, emb: List<Float32>)` parquet into a flat `(num_rows, Vec<f32>)`.
///
/// Rows are returned in the file's natural order (which for VectorDBBench's
/// bundles matches the `id` column 0..num_rows-1).
///
/// `expected_dim` is asserted against the first row; mismatch returns
/// [`VbenchError::InvalidInput`]. This catches the common mistake of feeding
/// a 1024d dataset's parquet to a 768d-configured run.
pub fn read_embeddings_parquet(path: &Path, expected_dim: usize) -> Result<(usize, Vec<f32>)> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| VbenchError::InvalidInput(format!("parquet open: {e}")))?;
    let reader = builder
        .build()
        .map_err(|e| VbenchError::InvalidInput(format!("parquet build: {e}")))?;

    let mut flat: Vec<f32> = Vec::new();
    let mut total_rows: usize = 0;
    for batch in reader {
        let batch = batch.map_err(|e| VbenchError::InvalidInput(format!("parquet read: {e}")))?;
        let emb_col = locate_embedding_column(&batch)?;
        decode_embedding_column(emb_col, expected_dim, &mut flat, &mut total_rows)?;
    }
    Ok((total_rows, flat))
}

/// Decode a `(id, neighbors_id: List<Int64>)` parquet into `Vec<Vec<u64>>`.
///
/// Each entry is the ground-truth neighbour ids for one query, in score
/// order (best first). Lengths are not validated against `expected_neighbors`
/// — the runner only ever uses the first `recall_k` of each list, so a
/// dataset with more than expected neighbours per row is not a bug.
pub fn read_neighbours_parquet(path: &Path) -> Result<Vec<Vec<u64>>> {
    let file = File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| VbenchError::InvalidInput(format!("parquet open: {e}")))?;
    let reader = builder
        .build()
        .map_err(|e| VbenchError::InvalidInput(format!("parquet build: {e}")))?;

    let mut out: Vec<Vec<u64>> = Vec::new();
    for batch in reader {
        let batch = batch.map_err(|e| VbenchError::InvalidInput(format!("parquet read: {e}")))?;
        let col = locate_neighbours_column(&batch)?;
        let list = col.as_any().downcast_ref::<ListArray>().ok_or_else(|| {
            VbenchError::InvalidInput("neighbors_id column is not a List<Int64>".to_string())
        })?;
        for i in 0..list.len() {
            let inner = list.value(i);
            let i64s = inner.as_any().downcast_ref::<Int64Array>().ok_or_else(|| {
                VbenchError::InvalidInput("neighbors_id inner column is not Int64".to_string())
            })?;
            // Cast i64 → u64. Negative ids would be a malformed dataset.
            let mut row: Vec<u64> = Vec::with_capacity(i64s.len());
            for j in 0..i64s.len() {
                let v = i64s.value(j);
                if v < 0 {
                    return Err(VbenchError::InvalidInput(format!(
                        "negative neighbour id at row {i}: {v}"
                    )));
                }
                row.push(v as u64);
            }
            out.push(row);
        }
    }
    Ok(out)
}

fn locate_embedding_column(batch: &RecordBatch) -> Result<&dyn Array> {
    // Prefer the conventional name "emb"; fall back to "embedding" for
    // compatibility with bundles that use the longer name.
    for name in ["emb", "embedding"] {
        if let Some(col) = batch.column_by_name(name) {
            return Ok(col.as_ref());
        }
    }
    Err(VbenchError::InvalidInput(
        "no 'emb' or 'embedding' column in parquet".to_string(),
    ))
}

fn locate_neighbours_column(batch: &RecordBatch) -> Result<&dyn Array> {
    for name in ["neighbors_id", "neighbours_id", "neighbors"] {
        if let Some(col) = batch.column_by_name(name) {
            return Ok(col.as_ref());
        }
    }
    Err(VbenchError::InvalidInput(
        "no 'neighbors_id' / 'neighbours_id' / 'neighbors' column".to_string(),
    ))
}

fn decode_embedding_column(
    col: &dyn Array,
    expected_dim: usize,
    flat: &mut Vec<f32>,
    total_rows: &mut usize,
) -> Result<()> {
    if let Some(list) = col.as_any().downcast_ref::<ListArray>() {
        for i in 0..list.len() {
            let inner = list.value(i);
            let f32s = inner
                .as_any()
                .downcast_ref::<arrow::array::Float32Array>()
                .ok_or_else(|| {
                    VbenchError::InvalidInput("embedding inner column is not Float32".to_string())
                })?;
            if f32s.len() != expected_dim {
                return Err(VbenchError::InvalidInput(format!(
                    "row {i}: dim {} but expected {expected_dim}",
                    f32s.len()
                )));
            }
            flat.reserve(expected_dim);
            for j in 0..expected_dim {
                flat.push(f32s.value(j));
            }
            *total_rows += 1;
        }
        return Ok(());
    }
    if let Some(fl) = col.as_any().downcast_ref::<FixedSizeListArray>() {
        let inner_len = fl.value_length() as usize;
        if inner_len != expected_dim {
            return Err(VbenchError::InvalidInput(format!(
                "FixedSizeList width {inner_len} != expected_dim {expected_dim}",
            )));
        }
        for i in 0..fl.len() {
            let inner = fl.value(i);
            let f32s = inner
                .as_any()
                .downcast_ref::<arrow::array::Float32Array>()
                .ok_or_else(|| {
                    VbenchError::InvalidInput("embedding inner column is not Float32".to_string())
                })?;
            flat.reserve(expected_dim);
            for j in 0..expected_dim {
                flat.push(f32s.value(j));
            }
            *total_rows += 1;
        }
        return Ok(());
    }
    Err(VbenchError::InvalidInput(
        "embedding column is neither ListArray nor FixedSizeListArray".to_string(),
    ))
}
