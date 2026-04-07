# Adapters

Adding a new vector database to vbench is one trait impl. This document
walks through the trait shape, the conventions every adapter follows,
and how to wire a new adapter into the CLI.

## The `BenchAdapter` trait

Defined in `crates/vbench-core/src/adapter.rs`.

```rust
#[async_trait]
pub trait BenchAdapter: Send + Sync {
    fn info(&self) -> AdapterInfo;

    async fn open(
        workdir: &Path,
        dim: usize,
        metric: Metric,
        params: &serde_json::Value,
    ) -> anyhow::Result<Self>
    where Self: Sized;

    async fn load(&self, rows: &[VectorRow<'_>]) -> anyhow::Result<()>;

    async fn optimize(&self) -> anyhow::Result<()> { Ok(()) }

    async fn search(&self, query: &[f32], k: usize) -> anyhow::Result<Vec<u64>>;

    fn supports_filtered_search(&self) -> bool { false }

    async fn search_filtered(
        &self,
        _query: &[f32],
        _k: usize,
        _filter: &serde_json::Value,
    ) -> anyhow::Result<Vec<u64>> {
        anyhow::bail!("filtered search is not supported by this adapter")
    }

    async fn count(&self) -> anyhow::Result<u64>;

    async fn shutdown(self) -> anyhow::Result<()>
    where Self: Sized;
}
```

### Lifecycle

The runner calls these in this order, exactly once per run:

1. `open(workdir, dim, metric, params)` — provision the DB and any
   on-disk state. The runner hands you a fresh tempdir; you don't need
   to wipe it.
2. `load(rows)` — called repeatedly with batches of `--batch-size`
   `VectorRow`s. The runner concatenates the dataset's training vectors
   into these batches in id order.
3. `count()` — runner asserts this equals the dataset's `num_train`
   after the load phase finishes. Mismatch fails the run.
4. `optimize()` — adapter-specific warm-up. The runner calls your
   `optimize()` then immediately issues `--warmup-queries` real test
   queries — both are timed together as `optimize_duration`.
5. `search(query, k)` — called once per test query. The runner times
   each call individually and feeds the latency into an HDR histogram.
6. `shutdown(self)` — consumes self. Drop any background processes,
   close connections, release locks.

### `open()` parameters

- `workdir: &Path` — owned by the runner. The adapter is free to write
  data files here. The runner removes it on exit (unless
  `--keep-workdir` was passed).
- `dim: usize` — the dataset's vector dimensionality. The adapter must
  reject mismatched query dims at `search()` time.
- `metric: vbench_core::Metric` — `Cosine`, `L2`, or `Ip`. The adapter
  is responsible for mapping this to whatever metric type its DB uses.
- `params: &serde_json::Value` — opaque adapter-specific config. Each
  adapter documents the schema it expects in its README. vbench-core
  never touches the contents.

### `info()` and `db_version`

`AdapterInfo::db_version` should be **runtime-detected**, not a
build-time constant. For Strata that means the version returned by the
IPC `Ping` command, set during `open()` and stored on the adapter
struct. The runner copies this into `db_config.version` of the published
result so reviewers can attribute drift to a specific DB release.

### `count()`

The runner uses `count()` only as a "did the load finish" sanity check.
Phase 1 adapters typically maintain an `AtomicU64` counter that's
incremented in `load()` and read here. Phase 2+ adapters that need to
sanity-check against the DB's own count can round-trip via a stats RPC.

### `optimize()`

Default: no-op. Override only if your DB needs an explicit warm-up
beyond the runner's `--warmup-queries` loop. The Strata adapter's
`optimize()` is a no-op because Strata's HNSW build is lazy and the
runner's warmup loop forces it via real queries anyway.

### `search()`

Returns the matched row ids in score order (best first). Don't return
duplicates — `recall_at_k` and `ndcg_at_k` deduplicate via a `HashSet`
but treat that as a defensive measure, not a feature.

If the adapter's native search returns more than `k` results, truncate
before returning. If it returns fewer, return what you have — the
runner divides by `k` (the constant), not by the number of returned
results.

### `shutdown()`

Consumes `self` so the runner can't accidentally reuse a half-closed
adapter. Adapters that spawn background processes (e.g.
`vbench-strata` spawns `strata up`) MUST kill them here. Use
`tokio::process::Command::kill_on_drop(true)` as a belt-and-braces
fallback against panics.

## Cargo wiring

Each adapter is its own crate under `crates/vbench-<dbname>/`. The CLI
gates each adapter behind a Cargo feature so users can build a slim
binary with only the DBs they care about.

```toml
# crates/vbench-cli/Cargo.toml
[features]
default = ["strata"]
strata = ["dep:vbench-strata"]
qdrant = ["dep:vbench-qdrant"]      # Phase 2
all-adapters = ["strata", "qdrant"] # Phase 2
```

The CLI's `cmd_run.rs` dispatches on the `--adapter <name>` arg with
`#[cfg(feature = "<name>")]` arms.

## Worked example: `vbench-strata`

`crates/vbench-strata/` is the reference adapter. It's intentionally
unusual because it drives a precompiled binary as a child process
rather than linking the DB as a library. Most adapters won't need to
do that.

The structure:

```
crates/vbench-strata/
├── Cargo.toml
├── src/
│   ├── lib.rs           # re-exports
│   ├── locate.rs        # find the strata binary
│   └── adapter.rs       # the BenchAdapter impl
└── tests/
    └── smoke.rs         # #[ignore]'d e2e against a real strata daemon
```

Read [`docs/strata-binary.md`](strata-binary.md) for the IPC details.

## Contribution flow for a new adapter

1. Open an issue describing which DB and roughly how it'll be wired
   (library vs server vs binary subprocess).
2. Create `crates/vbench-<dbname>/` with the same skeleton as
   `vbench-strata`.
3. Implement the trait. Start with the smoke test (a tiny in-process
   round-trip). Don't bother optimising — get correctness first.
4. Add a Cargo feature in `crates/vbench-cli/Cargo.toml`. Wire the
   `cmd_run.rs` dispatch.
5. Run `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
6. Open a PR. Include the smoke-test output against the real DB binary.
7. Bonus: a published Cohere-1M result alongside the PR.
