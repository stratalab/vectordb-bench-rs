# Methodology

`vbench` mirrors VectorDBBench upstream's methodology so the numbers we
publish are directly comparable to the existing leaderboard. This document
describes each phase, the metric formulas, the units, and the deliberate
design choices.

## Phases

A vbench run drives the adapter through four phases in order:

### 1. insert

Streams the dataset's training vectors into the adapter in batches of
`--batch-size` (default 1000). Wall-clock time is recorded as
`metrics.insert_duration` (seconds).

After the insert phase the runner calls `BenchAdapter::count()` and
asserts that it equals the dataset's `num_train`. A mismatch fails the
run with `VbenchError::InvalidInput("post-load count mismatch: …")`.

### 2. optimize

Calls `BenchAdapter::optimize()` followed by `--warmup-queries` (default
200) real test queries from the dataset. The combined wall-clock is
recorded as `metrics.optimize_duration` (seconds).

The warm-up loop is what forces lazy index builds in adapters like
Strata, where HNSW construction happens on the first query rather than
at insert time. The Strata adapter's own `optimize()` is a no-op for
exactly this reason — the runner's warm-up loop does the work.

`metrics.load_duration = metrics.insert_duration + metrics.optimize_duration`,
matching upstream's invariant in
[`task_runner.py:_run_perf_case`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/backend/task_runner.py)
lines 201–203.

### 3. recall + serial latency (folded)

Serial loop over every test query. For each:

1. Start `Instant::now()`.
2. Call `adapter.search(query, recall_k)`.
3. Record the elapsed micros into an HDR histogram (1 µs..60 s, 3 sig figs).
4. Compare the returned ids against the dataset's ground truth top-k for
   that query.
5. Accumulate `recall_at_k` and `ndcg_at_k`.

After the loop, the histogram surfaces `serial_latency_p99` and
`serial_latency_p95` in **seconds** (not ms — see Units below).

### 4. concurrent QPS sweep

**Phase 2 — not yet implemented.** Tracked in
[issue #5](https://github.com/stratalab/vectordb-bench-rs/issues/5).

The result JSON's `conc_*` fields are present as empty defaults so the
schema parses cleanly through upstream's tooling.

## Metrics

All formulas are byte-compatible with upstream's
[`vectordb_bench/metric.py:calc_recall`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/metric.py)
and `calc_ndcg`. Verified by reading both files and pinning the behaviour
in `vbench-core/tests/recall.rs`.

### recall@k

```text
recall_at_k(actual, ground_truth_topk, k):
    truth_set = set(ground_truth_topk)
    hits = count of i in 0..k where actual[i] in truth_set
    return hits / k
```

Iterates `actual.iter().take(k)` and counts membership in
`ground_truth_topk` (which the caller has already truncated to length
k, matching upstream's `gt[: self.k]` slice). Divides by `k` (the
constant), **not** by `min(k, ground_truth.len())`. If the adapter
returns fewer than `k` results, the trailing positions don't contribute.

### ndcg@k

```text
ndcg_at_k(actual, ground_truth_topk, k):
    ideal_dcg = sum(1 / log2(i+2) for i in 0..k)
    dcg = 0
    for got_id in set(actual.iter().take(k)):
        if got_id in ground_truth_topk:
            idx = position of got_id in ground_truth_topk
            dcg += 1 / log2(idx + 2)
    return dcg / ideal_dcg
```

**Important**: this is NOT textbook NDCG. Upstream's `calc_ndcg` discounts
each found id by its **position in `ground_truth_topk`**, not its position
in `actual`. As a consequence, the score is **insensitive to the order of
items within `actual`** — getting the right ids back in any permutation
produces the same NDCG.

This is unusual but it is exactly what the leaderboard expects. We pin
the behaviour with `ndcg_order_insensitive` in
`vbench-core/tests/recall.rs`.

`actual` is also deduplicated via a `HashSet`, matching upstream's
`set(got)`.

### qps

```text
qps = (successful concurrent queries during the sweep) / (sweep wall-clock)
```

Set from the **concurrent** phase, not the serial phase. Phase 1 only
runs serial, so `qps` stays at the default `0.0` — same as upstream when
the concurrent stage is skipped.

## Units

These are the most common way to silently produce incomparable benchmark
numbers. Verified against the published
[`result_20260403_standard_elasticcloud.json`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/results/ElasticCloud/result_20260403_standard_elasticcloud.json):

| Field | Unit |
|---|---|
| `insert_duration`, `optimize_duration`, `load_duration` | **seconds** (f64) |
| `qps` | queries / second (f64) |
| `serial_latency_p99`, `serial_latency_p95` | **seconds** (f64), e.g. `0.0106` = 10.6 ms |
| `conc_latency_*_list` | **seconds** (Vec<f64>) |
| `recall`, `ndcg` | `[0.0, 1.0]` (f64) |

The `vbench-core/tests/result_schema.rs::schema_latency_units_are_seconds_not_ms`
test asserts the unit explicitly: a synthetic `serial_latency_p99 = 0.0106`
must round-trip as `0.0106`, not `10.6`.

## Defaults

| Knob | Default | Source |
|---|---|---|
| `--recall-k` | 100 | upstream's `K_DEFAULT` |
| `--batch-size` | 1000 | vbench-specific |
| `--warmup-queries` | 200 | vbench-specific |

The recall_k default of 100 matters: at recall@10 vs recall@100 you can
get materially different numbers from the same adapter, and the
leaderboard reports the @100 figure. Don't override unless you know what
you're doing.

## Determinism

vbench runs are **as deterministic as the underlying adapter**. The
dataset is loaded in the same order every run (parquet rows in id
order). Test queries are issued in the same order. Latency varies with
the host's CPU + memory state — vbench does not pin CPUs, raise
priorities, or otherwise control the OS scheduler.

For publishing numbers, run on a machine with no other workload, with
fixed governor settings (`cpupower frequency-set -g performance`), and
include the `HOST.md` sidecar describing the box.

## What we don't measure (yet)

- **Concurrent QPS sweep**: Phase 2 ([#5](https://github.com/stratalab/vectordb-bench-rs/issues/5))
- **Filtered search**: the `BenchAdapter::supports_filtered_search` capability
  probe exists but no adapter currently uses it
- **Streaming write+read interleaved cases**: upstream's `st_*` fields are
  present in our schema as empty defaults; populating them is a Phase 3+
  goal
- **Capacity cases**: `metrics.max_load_count` is always 0
- **Build / index memory**: not currently surfaced
- **Cold-cache vs warm-cache differentiation**: a single warm-up loop;
  the recall-phase numbers reflect a warm cache only

## References

- [VectorDBBench upstream repo](https://github.com/zilliztech/VectorDBBench)
- [`vectordb_bench/metric.py`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/metric.py) — recall and NDCG formulas
- [`vectordb_bench/models.py`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/models.py) — TestResult schema
- [`vectordb_bench/backend/runner/serial_runner.py`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/backend/runner/serial_runner.py) — phase orchestration
- [`vectordb_bench/backend/task_runner.py`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/backend/task_runner.py) — insert/optimize/load split
