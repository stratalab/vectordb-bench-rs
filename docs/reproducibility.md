# Reproducibility

This document is the recipe a reader follows to reproduce any
published vbench result. It also documents the host preparation steps
and the statistical methodology we expect submissions to use.

If you're publishing numbers, your submission's `REPRODUCE.md` is a
specialised version of what's in this document, pinned to your exact
versions and host.

## The recipe

For any result file `results/<date>/<adapter>-<dataset>/<adapter>-<dataset>.json`,
the steps are:

### 1. Pin versions

Read the result's `db_config.version` and the submission's
`REPRODUCE.md`. You need exact pins for **three** things:

- **vbench**: a git SHA, not a branch or tag
- **the DB under test**: a release tag (e.g. strata `v0.6.1`)
- **the dataset**: the catalog id and, ideally, a sha256 of the
  parquet files

### 2. Install vbench at the pinned SHA

```bash
cargo install --git https://github.com/stratalab/vectordb-bench-rs \
    --rev <git-sha> \
    vbench-cli
```

`--rev <git-sha>` (not `--tag` or `--branch`) is what makes this
reproducible across upstream changes.

### 3. Install the DB at the pinned version

For Strata:

```bash
curl -fsSL https://stratadb.org/install.sh | sh
strata --version  # confirm matches db_config.version in the result file
```

For other adapters, follow that adapter's documented install path
from its README.

### 4. Pull the dataset

```bash
vbench fetch <dataset-id>
```

vbench caches under `~/.cache/vectordb-bench-rs/datasets/<subdir>/`
with a `.complete` marker file. Partial downloads heal themselves on
re-run.

If the submission's `REPRODUCE.md` includes a sha256 for the parquet
files, verify it now:

```bash
cd ~/.cache/vectordb-bench-rs/datasets/cohere_medium_1m
sha256sum train.parquet test.parquet neighbors.parquet
```

### 5. Match the host class

Read the submission's `HOST.md`. The numbers will only match within
single-digit-percent if you run on a host of the same class:

- **CPU family** (Zen 4 vs Sapphire Rapids vs Apple M3 — all give
  different numbers)
- **Logical core count** (the recall phase is single-threaded today,
  but cache size and memory channels matter)
- **RAM** (the dataset has to fit; OOM with swap will tank latency)
- **Disk** (NVMe vs SATA SSD vs HDD changes load_duration by an
  order of magnitude)
- **OS** (Linux vs macOS, kernel version)
- **Governor settings** (see [host preparation](#host-preparation))

If you can't match the class, the comparison is qualitative, not
quantitative.

### 6. Run the same command N times

The result file's `task_config` records the exact knobs the original
submitter used. Replay them:

```bash
vbench run --adapter <adapter> --dataset <dataset> \
    --recall-k <k from task_config.case_config.k> \
    --batch-size <from task_config> \
    --warmup-queries <from task_config> \
    --task-label <your-label> \
    --output your-result.json
```

Run this **at least 3 times** after a single discarded warm-up run
(see [statistical methodology](#statistical-methodology)).

### 7. Verify

Compare your numbers against the published file:

| Field | Expected agreement |
|---|---|
| `recall` | within 0.5% (HNSW ties break differently across runs) |
| `ndcg` | within 0.5% (same reason) |
| `insert_duration` | within 10% |
| `optimize_duration` | within 20% |
| `load_duration` | within 10% |
| `serial_latency_p99` | within 25% |
| `serial_latency_p95` | within 20% |

If your numbers are outside these envelopes on a matched host class,
**something is different**. Either:

- A version is wrong (recheck #1–3)
- The host has a confounder (background workload, thermal throttling)
- The original submission has a methodology bug worth reporting via
  the dispute process in [`submitting-results.md`](submitting-results.md#disputes)

## Host preparation

Skip these and your numbers will be noisier than they need to be.

### Linux

```bash
# Set CPU governor to performance on every core
sudo cpupower frequency-set -g performance

# Or, if cpupower isn't installed:
for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
    echo performance | sudo tee "$cpu" > /dev/null
done

# Disable swap (optional but eliminates a confounder)
sudo swapoff -a

# Drop the page cache before each run (forces cold-disk reads of the parquet files)
sudo sync && echo 3 | sudo tee /proc/sys/vm/drop_caches > /dev/null
```

If you're on a battery-powered laptop, plug it in. Power-management
throttling will tank latency unpredictably.

### macOS

There's no CPU governor knob equivalent. The closest you can do:

- Plug into power
- Close every other app
- Run with the lid open (closing the lid throttles)

`pmset -g` shows the current power-management state. Note it in
`HOST.md`.

### Cloud instances

Document the instance type in `HOST.md`. Cloud instances have a
"noisy neighbour" problem that no preparation can eliminate — the
best you can do is run multiple times across multiple instances and
report the spread, not just the median.

## Statistical methodology

A single run is a signal, not a data point. Latency on a busy box can
swing by 30%+ across runs of the same workload. The published numbers
need to be **statistically meaningful**.

### Recommended protocol

1. **One discarded warm-up run.** Don't even record this. It's there
   to populate caches, fault in pages, JIT-compile anything, etc.
2. **Three measurement runs.** Same command, same workdir cleared
   between runs, no other workload on the box.
3. **Report the median by `serial_latency_p99`.** Median, not mean.
   Mean is too sensitive to a single outlier.
4. **Disclose the spread.** In `DISCLOSURE.md`, write the min/max/median
   for each headline number across the 3 runs. If the spread is
   wider than the agreement envelopes in the verification table
   above, your environment is too noisy and you should add more
   runs or fix the noise source before publishing.

### Acceptable shortcuts

- A single run is acceptable for **smoke tests** and **PR previews**
  but not for published results.
- "Best of N" is acceptable if explicitly disclosed and N is also
  disclosed. The leaderboard will rank "best of N" results below
  "median of N" results for the same N.
- Some adapters have first-query cold-start costs that the warm-up
  run doesn't eliminate. Discard the first 1% of recall-phase queries
  if so, and document it.

## Known sources of variance

Things that move numbers between runs of the same code on the same
host:

- **HNSW build determinism.** Most HNSW implementations are
  deterministic given a fixed seed, but tie-breaks between
  equidistant neighbours vary based on insertion order, which can
  vary based on parallel batch processing. Recall typically varies
  by ≤ 0.5% run-to-run for this reason.
- **Disk cache state.** The first recall query after a load hits
  cold disk for index pages. The 200-query warm-up loop usually
  amortises this, but on slow disks it doesn't fully.
- **Memory layout.** ASLR, allocator decisions, and CPU cache line
  alignment can shift latency by a few percent.
- **CPU thermal state.** A box that just finished a build is hotter
  than a freshly-booted one, and the thermal governor will throttle
  differently.
- **tokio task scheduling.** The runner is single-threaded today, so
  this is small, but Phase 2's concurrent sweep will be sensitive
  to it.
- **Network state.** Only matters for adapters that talk to remote
  DBs. The Strata adapter is local-only so this is irrelevant.

If you suspect a confounder, the cleanest debugging move is to run
both vbench and a known-stable workload (e.g. `sysbench --threads=1`)
back-to-back and check whether the stable workload's numbers also
shift. If they do, the host is the problem. If they don't, vbench is.

## Verification responses

If you reproduce a result and your numbers match within the agreement
envelopes, **submit a verification PR**. It's a regular result
submission with these tweaks:

- Task label includes the suffix `-verified-by-<your-name>`
- `DISCLOSURE.md` references the original result file
- `REPRODUCE.md` includes a side-by-side comparison table

Verified results land alongside the original in `results/` and the
leaderboard ranks them higher than unverified ones.

If your numbers **don't** match, follow the
[dispute process](submitting-results.md#disputes) instead of opening
a duplicate result PR.

## What we explicitly don't reproduce

Some things vary inherently and we don't try to control for them:

- **Wall-clock dates.** `timestamp` will be different on every run.
- **`run_id`.** A fresh UUID4 every run. The schema-drift test
  asserts the format, not the value.
- **Per-query latency distributions.** We only publish p99 and p95;
  the underlying histogram is not in the result file.

## Quick reference

```bash
# Reproduce in 5 commands (after install.sh and cargo install)
vbench fetch cohere-1m
sudo cpupower frequency-set -g performance
sudo sync && echo 3 | sudo tee /proc/sys/vm/drop_caches  # cold-cache run
for i in 1 2 3 4; do                                      # warm-up + 3 measurement runs
    vbench run --adapter strata --dataset cohere-1m \
        --task-label "verify-strata-cohere-1m-run-$i" \
        --output "run-$i.json"
done
vbench inspect run-2.json   # report the median (or compute it from the 3)
```
