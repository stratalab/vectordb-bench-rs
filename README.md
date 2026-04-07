# vectordb-bench-rs

A native-Rust vector database benchmark harness, modelled on
[VectorDBBench](https://github.com/zilliztech/VectorDBBench)'s methodology and
emitting results in its `TestResult` JSON schema so they can be dropped into the
existing leaderboard tooling.

## Why a Rust harness?

Both [VectorDBBench](https://github.com/zilliztech/VectorDBBench) (Zilliz) and
[vector-db-benchmark](https://github.com/qdrant/vector-db-benchmark) (Qdrant)
are Python-only and assume the database under test is reachable as a server over
HTTP or gRPC. There is no Rust-native equivalent, and no embedded vector DB has
ever been benchmarked through them — even Qdrant benchmarks itself over HTTP,
not via FFI.

That means embedded Rust vector DBs (Strata, LanceDB, oasysdb, …) cannot
publish credible numbers without paying Python-FFI overhead that destroys the
actual performance story.

`vectordb-bench-rs` fills the gap. It is:

- **Native Rust** — no Python in the hot path
- **Adapter-based** — adding a new DB means implementing the `BenchAdapter` trait
- **Faithful to upstream** — outputs match VectorDBBench's `TestResult` schema
  field-for-field, so leaderboard tooling can consume our numbers without
  knowing they came from a different harness

## Status

**Phase 1** — minimal viable harness with a Strata adapter. Single dataset
(Cohere-1M), serial recall + latency, no concurrent QPS sweep yet.

**Phase 2** (planned) — Qdrant adapter, concurrent QPS, more datasets.

Actively under development. Not yet released.

## License

Apache-2.0
