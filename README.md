# vectordb-bench-rs

**A native-Rust vector database benchmark harness with strict
[VectorDBBench](https://github.com/zilliztech/VectorDBBench) wire compatibility.**

[![CI](https://github.com/stratalab/vectordb-bench-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/stratalab/vectordb-bench-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`vbench` runs the same load → optimize → recall → latency phases as
VectorDBBench upstream, computes recall@k and NDCG@k with the same
algorithms, and emits a `TestResult` JSON document whose field names match
upstream's [`vectordb_bench/models.py:TestResult`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/models.py)
field-for-field — so reviewers can drop our numbers into the existing
leaderboard tooling without translation.

The first adapter targets the [Strata](https://stratadb.org) embedded vector
database. Adding a new DB is one trait impl.

## Why a Rust harness?

Both [VectorDBBench](https://github.com/zilliztech/VectorDBBench) (Zilliz) and
[vector-db-benchmark](https://github.com/qdrant/vector-db-benchmark) (Qdrant)
are Python-only and assume the database under test is reachable as a server
over HTTP or gRPC. There is no Rust-native equivalent, and no embedded vector
DB has ever been benchmarked through them — even Qdrant benchmarks itself
over HTTP, not via FFI.

That means embedded Rust vector DBs (Strata, LanceDB, oasysdb, …) cannot
publish credible numbers without paying Python-FFI overhead that destroys
the actual performance story.

`vectordb-bench-rs` fills the gap. It is:

- **Native Rust** — no Python in the hot path
- **Adapter-based** — adding a new DB is one `BenchAdapter` trait impl
- **Strict-schema** — outputs are byte-compatible with VectorDBBench's
  `TestResult` JSON, including units (latencies in seconds, not
  milliseconds) and the `case_id` enum mapping
- **Drives binaries, not source** — the Strata adapter consumes the
  precompiled binary delivered via `https://stratadb.org/install.sh`
  rather than rebuilding from source on every benchmark run

## Quick start

```bash
# 1. Install Strata (the adapter's dependency)
curl -fsSL https://stratadb.org/install.sh | sh

# 2. Install vbench
cargo install --git https://github.com/stratalab/vectordb-bench-rs vbench-cli

# 3. Download Cohere-1M (~3 GB; one-time)
vbench fetch cohere-1m

# 4. Run the benchmark
vbench run --adapter strata --dataset cohere-1m \
    --task-label "strata-0.6.1-cohere-1m" \
    --output strata-cohere-1m.json

# 5. Inspect the result
vbench inspect strata-cohere-1m.json
```

The output JSON drops directly into VectorDBBench's leaderboard tooling.

## What's measured

| Phase | What it does | Stored in |
|---|---|---|
| `insert` | Stream training vectors into the adapter in batches | `metrics.insert_duration` (seconds) |
| `optimize` | Adapter-specific warm-up + lazy index build | `metrics.optimize_duration` (seconds) |
| `recall` | Serial loop over test queries, compare top-k against ground truth | `metrics.recall`, `metrics.ndcg` |
| `serial latency` | Per-query latency captured in an HDR histogram | `metrics.serial_latency_p99`, `serial_latency_p95` (seconds) |

`load_duration = insert_duration + optimize_duration` (matches upstream).

The concurrent QPS sweep is **Phase 2** ([#5](https://github.com/stratalab/vectordb-bench-rs/issues/5)).
The fields are present in the result JSON as zero/empty defaults so the
schema parses cleanly.

See [`docs/methodology.md`](docs/methodology.md) for the full methodology and
the recall/NDCG formulas (which match upstream's, including the
order-insensitivity quirk in `calc_ndcg`).

## Datasets

| id | dim | metric | train | test | size | source |
|---|---|---|---|---|---|---|
| `cohere-1m` | 768 | cosine | 1,000,000 | 10,000 | ~3 GB | [Cohere medium 1M](https://huggingface.co/datasets/Cohere/wikipedia-22-12-en-embeddings) via Zilliz CDN |

More in [`docs/datasets.md`](docs/datasets.md).

## Adapters

| name | DB | status |
|---|---|---|
| `strata` | [Strata](https://stratadb.org) ≥ 0.6.1 | shipping |
| `qdrant` | [Qdrant](https://qdrant.tech) | Phase 2 |
| `instant-distance` | [instant-distance](https://github.com/InstantDomain/instant-distance) | Phase 2 |
| `lance` | [Lance](https://github.com/lancedb/lance) | proposed |
| `oasysdb` | [OasysDB](https://github.com/oasysai/oasysdb) | proposed |

The Strata adapter never imports `strata-executor` source. It locates the
precompiled `strata` binary (via `STRATA_BIN`, `PATH`, or
`~/.strata/bin/strata`), spawns it as a child process via `strata up --fg
--db <workdir>`, and drives it over the unix-socket IPC daemon. See
[`docs/strata-binary.md`](docs/strata-binary.md) for the wire-protocol
details and [`docs/adapters.md`](docs/adapters.md) for the trait shape.

## Result format

vbench emits one `TestResult` JSON document per run. The schema is
**strictly compatible** with upstream's
`vectordb_bench/models.py:TestResult`, including:

- `run_id` (UUID4 hex, no dashes)
- `task_label`, `timestamp`
- `results: [CaseResult]` array (length 1 in single-DB runs)
- `CaseResult { metrics, task_config, label }`
- `Metric` with every upstream field (`insert_duration`,
  `optimize_duration`, `load_duration`, `qps`, `serial_latency_p99/p95`,
  `recall`, `ndcg`, plus all the `conc_*` and `st_*` fields as
  zero/empty defaults)
- `TaskConfig { db, db_config, db_case_config, case_config, stages,
  load_concurrency }`
- `CaseConfig { case_id, custom_case, k, concurrency_search_config }`,
  with `case_id = 5` for Cohere-1M (matches upstream's
  `CaseType.Performance768D1M`)
- `label = ":)" / "x" / "?"`

**Critical units**: durations in seconds, latencies in seconds (not
milliseconds — upstream's `serial_latency_p99: 0.0106` means 10.6 ms).
`vbench-core/tests/result_schema.rs` is the long-term guard against
schema drift.

Field-by-field mapping in [`docs/interop.md`](docs/interop.md).

## Repo layout

```
vectordb-bench-rs/
├── crates/
│   ├── vbench-core/         # adapter trait + dataset + runner + result
│   ├── vbench-strata-ipc/   # hand-rolled MessagePack IPC client
│   ├── vbench-strata/       # Strata adapter (uses vbench-strata-ipc)
│   └── vbench-cli/          # `vbench` binary
├── docs/                    # methodology / datasets / adapters / interop / strata-binary
└── results/                 # published benchmark results (one per run)
```

## Status

**Phase 1** is shipping. The harness, the Strata adapter, and the CLI all
work end-to-end against `strata 0.6.1`. We have not yet published an
official Cohere-1M result.

**Phase 2** ([#5](https://github.com/stratalab/vectordb-bench-rs/issues/5)):
concurrent QPS sweep, more datasets, more adapters.

Not yet released as `v0.1.0`. The first tagged release will accompany the
first published Cohere-1M result.

## Contributing

Adding a new adapter is the most useful contribution. See
[`docs/adapters.md`](docs/adapters.md) for the `BenchAdapter` trait shape
and a worked example.

## License

[Apache-2.0](LICENSE)
