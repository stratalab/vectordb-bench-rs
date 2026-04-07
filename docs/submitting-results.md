# Submitting a benchmark result

A benchmark harness is only as useful as the trust readers place in its
published numbers. This document defines the submission process every
result has to go through to land in `results/` and on the leaderboard.

It exists for two reasons:

1. **So third parties can submit results.** vbench is intended to be a
   community benchmark, not a vanity project for any single vendor.
2. **So vendor-submitted results are clearly marked.** When a DB
   vendor submits numbers for their own DB, that's allowed — but the
   submission has to be tagged as such, and it's expected to come with
   enough disclosure for an independent reader to spot any unfair
   methodology.

If you publish a number that came from vbench but skipped this process,
**don't call it a "vbench result"**. Call it "a number I produced with
vbench". Reviewers will tell the difference.

## Two submission tracks

### Track A: independent third party (preferred)

You ran the benchmark on your own hardware against a DB you don't
maintain. Open a PR against this repo with the artifacts described
below. Maintainers verify the submission and merge.

### Track B: vendor self-submission (allowed, tagged)

You're a DB vendor submitting numbers for your own DB. The same PR
process applies, but the result file lands under `results/<date>/<adapter>-<DB-version>.vendor.json`
(note the `.vendor.json` suffix), and the leaderboard renders it with
a "vendor-submitted" tag. The disclosure section in your PR has to
identify which company you work for and which version of your DB the
adapter targets.

Vendor submissions are not second-class — they're often the most
careful and the best-tuned. The tag exists because readers should be
able to weight the result against their own conflict-of-interest
intuition.

## Required artifacts

Every submission PR must include **all** of the following, under
`results/<YYYY-MM>/<adapter>-<dataset>/`:

```
results/2026-04/strata-cohere-1m/
├── strata-cohere-1m.json        # the TestResult document vbench produced
├── HOST.md                      # filled-in HOST.md template
├── REPRODUCE.md                 # exact commands; vbench version + DB version
└── DISCLOSURE.md                # conflict-of-interest disclosure
```

For vendor submissions, the JSON file is named `<adapter>-<dataset>.vendor.json`.

### `strata-cohere-1m.json` (or equivalent)

The unmodified `TestResult` JSON vbench produced. **Don't edit it**. The
schema-drift test in `vbench-core/tests/result_schema.rs` is what
validates this on PR merge — if you've hand-edited the file, the test
won't catch it but a reviewer will.

The required fields:

- `run_id` — the UUID4 hex vbench generated
- `task_label` — your free-form label, ideally `<adapter>-<DB-version>-<dataset>` (e.g. `strata-0.6.1-cohere-1m`)
- `timestamp` — unix epoch
- `results[0].metrics.recall` — non-zero
- `results[0].metrics.serial_latency_p99` — non-zero, in **seconds**
- `results[0].task_config.db` — your adapter id
- `results[0].task_config.db_config.version` — runtime-detected DB version

### `HOST.md`

The host snapshot. Use the [`results/HOST.md.template`](../results/HOST.md.template)
file as the starting point. Required fields:

- CPU model + base/boost frequency
- Physical cores / logical threads
- RAM (GB)
- Disk model + type (SSD/NVMe/HDD)
- OS distro + kernel version
- rustc version (`rustc --version`)
- vbench git SHA
- DB version
- CPU governor setting (`cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor` on Linux)
- Number of runs aggregated into the result and which statistic
  (median? best? single?)

Optional but encouraged: cooling notes, virtualisation, cloud instance
type, anything else that affects reproducibility.

### `REPRODUCE.md`

The exact commands a reader should run to reproduce your numbers.
Minimum:

````markdown
# Reproduce

## Environment

- vbench: `https://github.com/stratalab/vectordb-bench-rs.git` @ `<git-sha>`
- DB: strata 0.6.1 from `https://stratadb.org/install.sh`
- Dataset: `cohere-1m` (sha256 of `train.parquet`: `<sha>`)
- Host class: see `HOST.md`

## Steps

```bash
curl -fsSL https://stratadb.org/install.sh | sh
cargo install --git https://github.com/stratalab/vectordb-bench-rs --rev <git-sha> vbench-cli
vbench fetch cohere-1m
vbench run --adapter strata --dataset cohere-1m \
    --task-label strata-0.6.1-cohere-1m \
    --recall-k 100 \
    --batch-size 1000 \
    --warmup-queries 200 \
    --output strata-cohere-1m.json
```

I ran the above 3 times after a single discarded warm-up run. The
published JSON is the median run by `serial_latency_p99`.
````

The `<git-sha>` placeholders are real SHAs — not branch names — so the
reproducer pins to exactly the same code.

### `DISCLOSURE.md`

A short statement covering:

- **Who you are**: name + (if applicable) employer
- **Conflict of interest**: do you work on, contribute to, or get paid
  by the DB you benchmarked? If yes: which one and what role.
- **How many runs**: how many times you ran the benchmark, what
  statistic you picked, and whether any runs were discarded
- **Tuning**: did you change any DB-side knobs from defaults? If yes,
  list them
- **Anything else a reviewer should know**

There's no length requirement. A two-sentence honest disclosure is
worth more than a five-paragraph hedged one.

## Acceptance criteria

Maintainers reject submissions that fail any of:

1. **Required artifacts missing.** All four files must be present.
2. **Schema invalid.** The JSON must round-trip through
   `serde_json::from_str::<TestResult>` without error and pass the
   schema-drift guard.
3. **Numerical sanity.**
   - `recall ∈ (0.0, 1.0]`
   - `serial_latency_p99 > 0`
   - `serial_latency_p95 ≤ serial_latency_p99`
   - `load_duration ≈ insert_duration + optimize_duration` (within 1%
     to allow for rounding)
   - `task_config.case_config.case_id` matches the dataset's catalog entry
4. **DB version is real.** The `db_config.version` must correspond to
   a publicly available DB release.
5. **Disclosure is present and honest.** Vendor-submitted results
   without a vendor disclosure are rejected.
6. **Fairness compliance.** See [`fairness.md`](fairness.md) for the full
   list. Submissions that violate the rules (e.g. caching test queries,
   special-casing query vectors, disabling persistence to win on insert
   speed) are rejected even if the JSON parses.

## Verification

Maintainers verify each submission with one or more of:

- **Re-running** on a similar host class (when practical)
- **Reading the adapter** for any custom code paths the submission
  exercises (especially for new adapters)
- **Comparing** against existing submissions for the same DB to spot
  outliers
- **Checking** the `REPRODUCE.md` commands against what's actually in
  the JSON's `task_config`

A submission can be merged with a "verification pending" tag and
later promoted once a maintainer has independently re-run it.

## Multiple submissions per DB

Welcome and encouraged. Each submission should reflect a meaningfully
different setup (different hardware class, different DB version,
different tuning). The leaderboard renders all of them; readers pick
which one matters for their use case.

Don't submit ten copies of the same run with cosmetic differences.
That's noise and gets squashed in review.

## Disputes

If you believe a published number is wrong or unfair, open an issue
labelled `result-dispute` with:

- The result file you're disputing
- Your evidence (a counter-result, an audit of the adapter code, a
  reproduction attempt that produced different numbers)
- What you think should happen (re-run, retract, re-tag as
  vendor-submitted, …)

Disputes are decided by the maintainer team based on evidence, not
authority. The default outcome is more transparency, not less — if a
result is unclear, the response is to add disclosure, not to delete
the submission.

## Retractions

A submission can be retracted by its original author at any time, no
questions asked. To retract, open a PR removing the result directory
and add a one-line entry to `results/RETRACTIONS.md` (created on first
retraction) with the reason. The leaderboard hides retracted results
but the git history preserves them.

Maintainer-initiated retractions follow the dispute process above.

## Where the leaderboard lives

The leaderboard is the `results/` tree itself, plus whatever rendering
tooling consumes it. vbench is wire-compatible with VectorDBBench's
schema, so upstream's leaderboard tooling can render our files
directly. We may add our own renderer later.

## Questions

Open an issue with the `submission-process` label. There are no dumb
questions about benchmark methodology.
