# Fairness policy

A benchmark number is only meaningful if the conditions that produced
it are fair. This document defines what counts as fair tuning, what
counts as cheating, and how vbench enforces the line.

The goal is simple: **the published numbers should reflect what a
careful real user would see in production**, not what a benchmarking
specialist can extract by overfitting to the dataset.

If you're submitting a result, read [`submitting-results.md`](submitting-results.md)
first. This document is the rules; that one is the process.

## Allowed

These are encouraged. Document them in `db_case_config` (the
adapter-specific tuning bag in the result JSON) so reviewers can see
what was tuned.

- **Tuning index parameters** (HNSW `M`, `efConstruction`, `efSearch`;
  IVF `nlist`, `nprobe`; PQ codebook sizes; …) per dataset. Different
  datasets benefit from different parameters and that's part of how
  the DB is meant to be used.
- **Pre-warming the index.** The runner already does this via
  `--warmup-queries`. Adapters can also implement `optimize()` to
  trigger eager index builds, force-merge segments, etc.
- **Multiple runs** on the same configuration, reporting the median
  or best by a single chosen metric. Disclose which one in your
  submission.
- **Custom adapter code that uses the DB's most efficient documented
  API.** If your DB has a streaming bulk-insert API, use it. If it
  has a server-side `searchBatch`, use it.
- **Reasonable build flags** (`--release`, target-cpu features that a
  user would actually deploy). Don't use `-Ctarget-cpu=native` unless
  you'd recommend that to your users.
- **JIT warmup, JVM heap tuning, GC settings** — anything a competent
  operator would do.
- **Multiple result submissions for different tuning profiles.** A
  "fastest" tuning and a "highest recall" tuning are both legitimate
  data points. Submit them separately with distinct task labels.

## Not allowed

These violate the fairness policy. Submissions that exhibit any of
these are rejected.

### Test-set leakage

- **Caching test queries between runs.** Each query in the recall
  phase must hit the live index, not a memoised result.
- **Pre-loading test query results.** The adapter must not have
  side-channel access to the ground-truth file.
- **Special-casing query vectors.** Don't detect that a query came
  from `test.parquet` and short-circuit to the ground-truth answer.
- **Using ground-truth in the search path.** The adapter must not
  read `neighbors.parquet` at any point during `load`, `optimize`,
  or `search`.
- **Indexing the test queries during load.** The training and test
  sets are disjoint by construction in upstream's bundles. Don't
  blur them.

### Skipping safety

- **Disabling persistence/durability/MVCC to win on insert speed.**
  The DB must run in a configuration a real user would deploy. If
  your DB has a `--unsafe-no-fsync` mode, don't use it. If durability
  is opt-in by default, that's fine — but document it in
  `db_case_config`.
- **Disabling crash recovery / WAL replay.** Same reason.
- **Skipping the `count()` check.** The runner asserts that the
  adapter holds `num_train` rows after the load phase. Adapters that
  return the expected count without actually loading the rows are
  cheating.

### Methodology games

- **Cherry-picking only the best run** from many while reporting it
  as a single number. If you ran 10 times and pick the fastest, say
  so in the disclosure ("best of 10"). Don't pretend it was a single
  run.
- **Hiding the build configuration.** The DB version, vbench version,
  build features, and adapter version must all be in the submission.
- **Tuning to the dataset's specific test queries.** Tuning to the
  dataset's *characteristics* (dim, distribution, size) is fine.
  Tuning to the *exact* test queries is overfitting.
- **Modifying the dataset.** Don't filter, downsample, deduplicate,
  or augment the parquet files. Use them as Zilliz hosts them.
- **Skipping the warm-up.** The optimize phase exists so the recall
  phase measures warm-cache performance. Adapters that do interesting
  work outside the optimize phase but call it `optimize` are
  misleading.

### Schema games

- **Hand-editing the result JSON.** The schema-drift test catches
  obvious changes; reviewers catch the rest. If your numbers don't
  look right, fix the bug, don't fix the JSON.
- **Reporting in the wrong units.** Latencies are in seconds (matching
  upstream), not milliseconds. The schema-drift guard test pins this.
- **Setting `qps` from the serial phase.** `qps` is the max QPS from
  the concurrent sweep (Phase 2). Phase 1 results must leave it at
  `0.0`.

## Adapter fairness checklist

Run through this when adding a new adapter or auditing an existing
one. Every "yes" is a tick toward a fair adapter.

- [ ] Uses default DB feature flags unless documented otherwise
- [ ] Uses the DB's documented bulk-insert API for `load()`
- [ ] Uses the DB's documented k-NN API for `search()`
- [ ] Doesn't read the dataset's `neighbors.parquet` at any point
- [ ] Doesn't memoise query → result mappings between calls
- [ ] `count()` reflects actual loaded rows, not expected ones
- [ ] `optimize()` is either a no-op or does work that a production
      user would do (warm-up, force-merge, etc.) — not a sneaky way
      to amortise work that should be in `load()` or `search()`
- [ ] `shutdown()` cleanly closes connections / kills child processes
- [ ] DB-side persistence/durability is in its production-default mode
- [ ] All non-default tuning is recorded in `db_case_config`

## How vbench enforces this

Three layers:

1. **Schema-drift guards** (`vbench-core/tests/result_schema.rs`)
   catch units, field renames, missing fields. Run on every PR.
2. **Numerical sanity checks** in
   [`submitting-results.md`'s acceptance criteria](submitting-results.md#acceptance-criteria)
   — recall in (0, 1], load_duration invariant, etc.
3. **Adapter audit on merge.** When a new adapter or a non-trivial
   adapter change lands, a maintainer reads the code looking for
   anything in this document's "not allowed" list. If something looks
   suspicious, the PR is held until the author can explain it.

Layer 3 is human and slow but it's the only thing that catches
genuine cheating. The first two layers catch honest mistakes.

## Vendor-submitted results

Vendor results are allowed and tagged. The fairness rules apply
identically — being the DB's vendor doesn't grant extra latitude. The
tag exists so that readers can apply their own confirmation-bias
discount, not so that the rules can be relaxed.

A vendor submission that violates the fairness rules is rejected
exactly the same way a third-party submission is. The only difference
is the suffix on the JSON file (`*.vendor.json`) and the requirement
that `DISCLOSURE.md` identifies the company.

## Independent verification

The single best protection against any form of unfairness is **a
re-run by a third party on similar hardware**. We encourage every
significant result to attract at least one independent reproduction
in `results/`.

Re-runs are themselves regular submissions and follow the
[submission process](submitting-results.md). They typically use a
task label like `<original-label>-verified-by-<name>` and reference
the original result file in their `DISCLOSURE.md`.

A result that's been verified by an independent submitter outranks
one that hasn't, in the leaderboard's ordering policy. (To be
implemented when we add a leaderboard renderer.)

## When in doubt

Document the choice and submit anyway. The maintainer team would
rather have a transparent borderline submission than a polished one
that hides decisions.

If you're not sure whether something counts as cheating, open a
discussion issue *before* you submit. We'd rather sort it out in the
abstract than reject your PR.

## Updates to this policy

Material changes to the fairness policy require:

1. A discussion issue with the `policy-change` label
2. At least 2 weeks of comment time
3. Sign-off from at least 2 maintainers
4. A `CHANGELOG.md` entry

We don't change the rules retroactively. A result that was fair
under the policy at the time of submission stays fair even if the
policy tightens later.
