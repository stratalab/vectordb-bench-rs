# Datasets

vbench ships a small built-in catalog of datasets, every entry of which
matches the layout used by the
[VectorDBBench-hosted bundles on the Zilliz CDN](https://assets.zilliz.com/benchmark/).
Each dataset is identified by a stable string id used by the CLI's
`--dataset` flag.

## Catalog

| id | dim | metric | train | test | neighbours/query | size | upstream `case_id` |
|---|---|---|---|---|---|---|---|
| `cohere-1m` | 768 | cosine | 1,000,000 | 10,000 | 100 | ~3 GB | `5` (`Performance768D1M`) |

The `case_id` column maps to upstream's
[`vectordb_bench/backend/cases.py:CaseType`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/backend/cases.py)
enum. Setting it correctly means our published results land in the
leaderboard's existing slot for that case rather than a separate "custom"
bucket.

## Bundle layout

All hosted bundles use the same three-file layout:

```
<base_url>/
  train.parquet      # id: int64, emb: List<Float32>
  test.parquet       # id: int64, emb: List<Float32>
  neighbors.parquet  # id: int64, neighbors_id: List<Int64>
```

`vbench-core/src/parquet_io.rs` decodes both `ListArray<Float32>` and
`FixedSizeListArray<Float32>` for the embedding columns, so a bundle that
ever switches to fixed-size lists won't break us.

## On-disk cache

Downloaded datasets are cached at:

```
$HOME/.cache/vectordb-bench-rs/datasets/<spec.cache_subdir>/
  train.parquet
  test.parquet
  neighbors.parquet
  .complete                # marker file written after every required file lands
```

The `.complete` marker is the **only** signal `ensure_dataset_downloaded`
trusts. Partial downloads (process killed mid-stream) leave a directory
without the marker, and the next run heals itself by re-downloading.

Override the cache root with `--cache-dir` on any subcommand. Inspect
the cache with `vbench cache show`. Wipe it with `vbench cache clear`.

## Adding a dataset

1. Add a new entry to `CATALOG` in `crates/vbench-core/src/dataset.rs`.
2. Set `case_id` to the matching upstream `CaseType` enum value, or `100`
   (`CaseType.Custom`) if there's no good fit.
3. Confirm the bundle is hosted somewhere stable and HTTP(S)-accessible.
4. The downloader uses `reqwest` with streaming, so range-request support
   isn't required, but the host should serve `Content-Length` for the
   progress display to work.
5. Optionally bump `approx_download_bytes` so `vbench fetch` can warn the
   user before pulling several GB.

## Bundles we may add later

Phase 2+ candidates, in rough priority order:

- **OpenAI-1M** (1536d, cosine): same Zilliz CDN, `case_id = 10`
  (`Performance1536D500K`)
- **GIST-1M** (960d, L2): standard ANN benchmark, may need a fresh
  hosted bundle if Zilliz doesn't carry it
- **SIFT-1M** (128d, L2): another standard
- **LAION-100M** (768d, cosine): the big one, needs the streaming-load
  + capacity-case path

PRs welcome — see
[`docs/adapters.md`](adapters.md) for the trait shape and the
contribution flow.
