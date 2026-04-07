//! Dataset catalog and the in-memory representation handed to the runner.
//!
//! Phase 1 ships exactly one entry: `cohere-1m`. New datasets are added by
//! appending to [`CATALOG`] and (typically) re-using the
//! `train.parquet`/`test.parquet`/`neighbors.parquet` schema convention used
//! by VectorDBBench's hosted bundles.
//!
//! ## Memory layout
//!
//! [`LoadedDataset`] stores both train and test embeddings as a flat
//! `Vec<f32>` (`num * dim` entries), then exposes them as iterators of
//! `&[f32]` slices. This avoids per-row `Vec` overhead and is friendlier
//! to the CPU cache than a `Vec<Vec<f32>>`. Cohere-1M at 768d × 1M is
//! ~3 GB resident — fine on a developer box, and the same as VectorDBBench's
//! Python loader uses.

use serde::{Deserialize, Serialize};

use crate::adapter::Metric;
use crate::error::{Result, VbenchError};

/// A static description of a dataset and where to fetch it.
///
/// Phase 1 only embeds Cohere-1M. The schema convention assumes the
/// VectorDBBench hosted bundles' layout:
///
/// - `train.parquet`: `id: int64, emb: List<Float32>`
/// - `test.parquet`:  `id: int64, emb: List<Float32>`
/// - `neighbors.parquet`: `id: int64, neighbors_id: List<Int64>`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSpec {
    /// Stable id used by the CLI's `--dataset` flag.
    pub id: &'static str,
    /// Human-readable name (used in result JSON's `case_config.dataset`).
    pub display_name: &'static str,
    /// Vector dimensionality.
    pub dim: usize,
    /// Distance metric the dataset is curated against.
    pub metric: Metric,
    /// Number of training (corpus) vectors.
    pub num_train: usize,
    /// Number of test queries.
    pub num_test: usize,
    /// Number of ground-truth neighbours per query (the upper bound for
    /// `recall_k`).
    pub num_neighbors: usize,
    /// Base URL of the hosted bundle. Filenames are appended.
    pub url_base: &'static str,
    /// Filename of the training parquet under `url_base`.
    pub train_file: &'static str,
    /// Filename of the test queries parquet under `url_base`.
    pub test_file: &'static str,
    /// Filename of the ground-truth neighbours parquet under `url_base`.
    pub neighbors_file: &'static str,
    /// Subdirectory within the cache root where this dataset is stored.
    pub cache_subdir: &'static str,
    /// Approximate total download size (used by the CLI's `fetch` command
    /// to give the user a heads-up before pulling several GB).
    pub approx_download_bytes: u64,
}

/// Built-in dataset catalog.
///
/// Phase 1 lists exactly one dataset. Add new entries here as new
/// VectorDBBench-hosted datasets get adapter coverage.
pub const CATALOG: &[DatasetSpec] = &[DatasetSpec {
    id: "cohere-1m",
    display_name: "Cohere medium 1M (768d cosine)",
    dim: 768,
    metric: Metric::Cosine,
    num_train: 1_000_000,
    num_test: 10_000,
    num_neighbors: 100,
    url_base: "https://assets.zilliz.com/benchmark/cohere_medium_1m/",
    train_file: "train.parquet",
    test_file: "test.parquet",
    neighbors_file: "neighbors.parquet",
    cache_subdir: "cohere_medium_1m",
    approx_download_bytes: 3 * 1024 * 1024 * 1024,
}];

/// Look up a dataset by its `id`.
pub fn get_spec(id: &str) -> Option<&'static DatasetSpec> {
    CATALOG.iter().find(|s| s.id == id)
}

/// All vectors and ground truth for a single dataset, in memory.
///
/// Construction is the loader's job ([`crate::download::ensure_dataset_downloaded`]
/// fetches the parquet files; [`crate::parquet_io`] decodes them).
#[derive(Debug)]
pub struct LoadedDataset {
    /// The catalog entry that produced this dataset.
    pub spec: &'static DatasetSpec,
    /// Training vectors flattened: length `spec.num_train * spec.dim`.
    train_flat: Vec<f32>,
    /// Test query vectors flattened: length `spec.num_test * spec.dim`.
    test_flat: Vec<f32>,
    /// For each test query, the list of ground-truth neighbour ids.
    /// Outer length: `spec.num_test`. Inner length: `spec.num_neighbors`.
    ground_truth: Vec<Vec<u64>>,
}

impl LoadedDataset {
    /// Construct a `LoadedDataset` from already-decoded buffers.
    ///
    /// Validates that lengths match the spec. Returns
    /// [`VbenchError::InvalidInput`] on mismatch.
    pub fn from_buffers(
        spec: &'static DatasetSpec,
        train_flat: Vec<f32>,
        test_flat: Vec<f32>,
        ground_truth: Vec<Vec<u64>>,
    ) -> Result<Self> {
        let expected_train = spec.num_train * spec.dim;
        if train_flat.len() != expected_train {
            return Err(VbenchError::InvalidInput(format!(
                "{}: train buffer has {} f32s, expected {}",
                spec.id,
                train_flat.len(),
                expected_train
            )));
        }
        let expected_test = spec.num_test * spec.dim;
        if test_flat.len() != expected_test {
            return Err(VbenchError::InvalidInput(format!(
                "{}: test buffer has {} f32s, expected {}",
                spec.id,
                test_flat.len(),
                expected_test
            )));
        }
        if ground_truth.len() != spec.num_test {
            return Err(VbenchError::InvalidInput(format!(
                "{}: ground truth has {} entries, expected {}",
                spec.id,
                ground_truth.len(),
                spec.num_test
            )));
        }
        Ok(Self {
            spec,
            train_flat,
            test_flat,
            ground_truth,
        })
    }

    /// Iterate training rows as `(id, &[f32])` pairs in id order.
    pub fn train_iter(&self) -> impl Iterator<Item = (u64, &[f32])> {
        self.train_flat
            .chunks_exact(self.spec.dim)
            .enumerate()
            .map(|(i, v)| (i as u64, v))
    }

    /// Iterate test queries as `(query_index, &[f32])` pairs.
    pub fn test_iter(&self) -> impl Iterator<Item = (usize, &[f32])> {
        self.test_flat.chunks_exact(self.spec.dim).enumerate()
    }

    /// Ground-truth neighbours for query at index `i`.
    pub fn ground_truth_for(&self, i: usize) -> &[u64] {
        &self.ground_truth[i]
    }

    /// Total bytes of resident dataset memory (rough estimate, ignoring
    /// `Vec` overhead).
    pub fn memory_bytes(&self) -> usize {
        self.train_flat.len() * std::mem::size_of::<f32>()
            + self.test_flat.len() * std::mem::size_of::<f32>()
            + self.ground_truth.iter().map(|v| v.len() * 8).sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohere_1m_resolves() {
        let spec = get_spec("cohere-1m").expect("cohere-1m in catalog");
        assert_eq!(spec.dim, 768);
        assert_eq!(spec.metric, Metric::Cosine);
        assert_eq!(spec.num_train, 1_000_000);
        assert_eq!(spec.num_test, 10_000);
    }

    #[test]
    fn unknown_dataset_returns_none() {
        assert!(get_spec("does-not-exist").is_none());
    }

    #[test]
    fn from_buffers_validates_train_len() {
        let spec = get_spec("cohere-1m").unwrap();
        // Wrong train length should fail.
        let bad = LoadedDataset::from_buffers(spec, vec![0.0; 10], vec![], vec![]);
        assert!(matches!(bad, Err(VbenchError::InvalidInput(_))));
    }

    #[test]
    fn from_buffers_validates_test_len() {
        let spec = &CATALOG[0];
        let train = vec![0.0_f32; spec.num_train * spec.dim];
        // Wrong test length should fail.
        let bad = LoadedDataset::from_buffers(spec, train, vec![0.0; 5], vec![]);
        assert!(matches!(bad, Err(VbenchError::InvalidInput(_))));
    }

    #[test]
    fn iter_train_yields_correct_dim_slices() {
        // Hand-built tiny dataset to keep the test fast.
        static TINY: DatasetSpec = DatasetSpec {
            id: "tiny",
            display_name: "tiny",
            dim: 4,
            metric: Metric::Cosine,
            num_train: 3,
            num_test: 1,
            num_neighbors: 1,
            url_base: "",
            train_file: "",
            test_file: "",
            neighbors_file: "",
            cache_subdir: "tiny",
            approx_download_bytes: 0,
        };
        let train_flat = vec![
            1.0, 0.0, 0.0, 0.0, // row 0
            0.0, 1.0, 0.0, 0.0, // row 1
            0.0, 0.0, 1.0, 0.0, // row 2
        ];
        let test_flat = vec![1.0, 0.0, 0.0, 0.0];
        let truth = vec![vec![0]];
        let ds = LoadedDataset::from_buffers(&TINY, train_flat, test_flat, truth).unwrap();

        let rows: Vec<_> = ds.train_iter().collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, 0);
        assert_eq!(rows[0].1, &[1.0, 0.0, 0.0, 0.0]);
        assert_eq!(rows[2].0, 2);
        assert_eq!(rows[2].1, &[0.0, 0.0, 1.0, 0.0]);
    }
}
