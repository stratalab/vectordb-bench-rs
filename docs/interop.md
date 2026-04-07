# Upstream wire interop

`vbench` emits a `TestResult` JSON document whose schema is **byte-compatible**
with VectorDBBench upstream's
[`vectordb_bench/models.py:TestResult`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/models.py).
The leaderboard tooling reads our files without translation.

This document is the field-by-field map and the rationale for the
deliberate compatibility decisions.

## Top-level structure

```
TestResult
├── run_id        : str       # UUID4 hex, no dashes
├── task_label    : str       # free-form, surfaced in the leaderboard
├── results       : [CaseResult]
├── file_fmt      : str       # filename template; we don't actually use it
└── timestamp     : float     # unix epoch in seconds
```

`results` is a list because upstream supports multi-DB orchestrated runs
in a single document. Phase 1 of vbench always emits length 1 (one
adapter, one dataset, one run), but the container shape stays compatible.

## CaseResult

```
CaseResult
├── metrics       : Metric
├── task_config   : TaskConfig
└── label         : str       # ":)" / "x" / "?" — success / failure / out-of-range
```

The `label` field is the leaderboard's status indicator. vbench writes
`":)"` on success. Failure modes (the runner errors out before
writing the file at all) are represented by the absence of a result
file, not a `"x"` label — there's no current code path that produces
the `"x"` or `"?"` label, but the field is in the schema for parity.

## Metric

Every field from upstream's
[`vectordb_bench/metric.py:Metric`](https://github.com/zilliztech/VectorDBBench/blob/main/vectordb_bench/metric.py)
is present, in the same order, with the same units. Phase 1 only
populates a subset; the rest are zero/empty defaults.

| Field | vbench Phase 1 | Unit | Source |
|---|---|---|---|
| `max_load_count` | `0` | rows | capacity cases (Phase 3+) |
| `insert_duration` | populated | seconds | insert phase wall-clock |
| `optimize_duration` | populated | seconds | optimize + warmup wall-clock |
| `load_duration` | populated | seconds | `insert_duration + optimize_duration` |
| `qps` | `0.0` | q/s | concurrent phase (Phase 2) |
| `serial_latency_p99` | populated | seconds | recall-phase HDR histogram |
| `serial_latency_p95` | populated | seconds | recall-phase HDR histogram |
| `recall` | populated | `[0,1]` | recall@k average |
| `ndcg` | populated | `[0,1]` | NDCG@k average |
| `conc_num_list` | `[]` | i32 list | Phase 2 |
| `conc_qps_list` | `[]` | f64 list | Phase 2 |
| `conc_latency_p99_list` | `[]` | f64 list | Phase 2 |
| `conc_latency_p95_list` | `[]` | f64 list | Phase 2 |
| `conc_latency_avg_list` | `[]` | f64 list | Phase 2 |
| `st_ideal_insert_duration` | `0` | i64 | streaming (Phase 3+) |
| `st_search_stage_list` | `[]` | i64 list | streaming |
| `st_search_time_list` | `[]` | f64 list | streaming |
| `st_max_qps_list_list` | `[]` | nested f64 list | streaming |
| `st_recall_list` | `[]` | f64 list | streaming |
| `st_ndcg_list` | `[]` | f64 list | streaming |
| `st_serial_latency_p99_list` | `[]` | f64 list | streaming |
| `st_serial_latency_p95_list` | `[]` | f64 list | streaming |
| `st_conc_failed_rate_list` | `[]` | f64 list | streaming |
| `st_conc_num_list_list` | `[]` | nested i32 list | streaming |
| `st_conc_qps_list_list` | `[]` | nested f64 list | streaming |
| `st_conc_latency_p99_list_list` | `[]` | nested f64 list | streaming |
| `st_conc_latency_p95_list_list` | `[]` | nested f64 list | streaming |
| `st_conc_latency_avg_list_list` | `[]` | nested f64 list | streaming |

## TaskConfig

```
TaskConfig
├── db                : str            # adapter id, e.g. "strata"
├── db_config         : object         # adapter-specific connection bag
├── db_case_config    : object         # adapter-specific tuning bag
├── case_config       : CaseConfig
├── stages            : [str]          # subset of ["drop_old","load","search_serial","search_concurrent"]
└── load_concurrency  : int            # Phase 1: 1
```

vbench populates:

- `db`: the adapter name (`"strata"`)
- `db_config`: `{ "db_label": <name>, "version": <runtime-detected>, "note": <free-form> }`
- `db_case_config`: `{ "metric_type": "COSINE" | "L2" | "IP" }`
- `case_config`: see below
- `stages`: `["drop_old", "load", "search_serial"]` for Phase 1; `"search_concurrent"` is added in Phase 2
- `load_concurrency`: `1`

Upstream's `db_config` carries DB-specific fields like `cloud_id` and
`password`; vbench's adapters don't need any of those, so they're
omitted (pydantic defaults to `extra='ignore'`, so absent fields don't
cause parse errors on the upstream reader).

## CaseConfig

```
CaseConfig
├── case_id                       : int
├── custom_case                   : object | null
├── k                             : int
└── concurrency_search_config     : ConcurrencySearchConfig
```

`case_id` maps to upstream's `CaseType` enum. vbench's catalog declares
the right `case_id` for each dataset:

| dataset | upstream `CaseType` | `case_id` |
|---|---|---|
| `cohere-1m` | `Performance768D1M` | `5` |

`k` is the runner's `--recall-k` (default 100, matching upstream's
`K_DEFAULT`).

`concurrency_search_config` defaults to upstream's standard sweep
levels: `[1, 5, 10, 20, 30, 40, 60, 80]`, 30s duration, 3600s timeout.
The fields are present even when no concurrent phase runs.

## Schema-drift guard

`crates/vbench-core/tests/result_schema.rs` is the long-term guard.
Ten tests cover:

- Top-level `TestResult` keys
- `CaseResult` keys
- Every `Metric` field present
- `TaskConfig` keys
- `CaseConfig` keys
- `ConcurrencySearchConfig` keys
- Full serde round-trip
- **Latency unit guard** — asserts a synthetic `0.0106` round-trips as `0.0106`, not `10.6`
- **`load_duration` invariant guard** — asserts `load == insert + optimize`
- **`run_id` format** — 32 hex chars, no dashes

If anyone renames a field on either side, these tests fail loudly
instead of producing silently-incomparable JSON.

## Drift attribution

Two fields exist specifically to make drift debuggable across releases:

- `db_config.version` — set from a runtime probe at adapter `open()`
  time, not a build-time constant. For Strata, this is the value
  returned by IPC `Ping`.
- `task_config.db` — the adapter id, so multi-DB orchestrators can
  group results.

When a vbench result and a leaderboard result diverge, start by
comparing these fields against each other.

## What we deliberately drop

A few extras vbench doesn't include in the result JSON:

- **Host snapshot** (CPU brand, RAM, kernel, rustc version) — lives in
  a sidecar `HOST.md` next to the result file, not in the JSON.
- **`vbench_schema_version`** — earlier drafts had this, but upstream
  doesn't, and pydantic's `extra='ignore'` means it would just be
  silently dropped on parse anyway.
- **Per-query latency mean / p50 / count** — upstream's `Metric` only
  carries p99 and p95 for the serial phase. vbench logs the others
  via `tracing` for our own debugging but doesn't put them in the JSON.

## Reading a vbench result file with upstream's tooling

```python
from vectordb_bench.models import TestResult

with open("strata-cohere-1m.json") as f:
    result = TestResult.parse_raw(f.read())

# Same as for any upstream-produced result.
print(result.results[0].metrics.recall)
print(result.results[0].metrics.serial_latency_p99 * 1000, "ms")
```

If `parse_raw` fails on a vbench-produced file, that's a wire-compat
bug and we want to know about it — please open an issue with the file
attached.
