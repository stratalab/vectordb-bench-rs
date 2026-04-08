<!--
Thanks for opening a PR.

Pick the section below that matches your change. Delete the sections
that don't apply. The reviewer is going to walk through the same
checklist, so completing it up front makes the review faster.
-->

## Type of change

<!-- Check exactly one. -->

- [ ] Bug fix
- [ ] New feature (harness, runner, CLI)
- [ ] New adapter (`crates/vbench-<dbname>/`)
- [ ] New dataset (entry in `CATALOG`)
- [ ] Result submission (third-party)
- [ ] Result submission (vendor — `*.vendor.json`)
- [ ] Verification of an existing result
- [ ] Documentation only
- [ ] Refactor / build / dependency update
- [ ] Methodology / fairness policy change

## Summary

<!-- 1–3 sentences describing what this PR does and why. -->

## Linked issues

<!-- "Fixes #N", "Closes #N", or just "Refs #N". -->

---

## Code-change checklist

<!-- Skip this section for result submissions and doc-only PRs. -->

- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo test --workspace` green
- [ ] If you touched `result.rs` or `metrics.rs`: schema-drift guard
      tests in `vbench-core/tests/result_schema.rs` updated and still
      passing
- [ ] If you touched a `BenchAdapter` impl: ran the live smoke test
      against a real DB binary (`STRATA_BIN=... cargo test -- --ignored`)
- [ ] Doc updates: any `docs/` file that documented the changed
      behaviour is updated in the same PR
- [ ] No new dependencies added without a reason in the PR body
- [ ] Backwards compatibility: if you touched the IPC wire mirror or
      the result schema, called out the impact explicitly

---

## Result submission checklist

<!-- Use this section ONLY for result-submission PRs. Delete it otherwise.
     Required reading: docs/submitting-results.md and docs/fairness.md. -->

### Required artifacts

All four files present under `results/<YYYY-MM>/<adapter>-<dataset>/`:

- [ ] `<adapter>-<dataset>.json` (or `*.vendor.json` for vendor submissions)
- [ ] `HOST.md` (filled in from `results/HOST.md.template`)
- [ ] `REPRODUCE.md` (exact commands, vbench git SHA, DB version)
- [ ] `DISCLOSURE.md` (who you are, COI, runs, tuning)

### Schema sanity

- [ ] `recall ∈ (0.0, 1.0]`
- [ ] `serial_latency_p99 > 0` and is in **seconds** (not ms)
- [ ] `serial_latency_p95 ≤ serial_latency_p99`
- [ ] `load_duration ≈ insert_duration + optimize_duration` (within 1%)
- [ ] `task_config.case_config.case_id` matches the dataset's catalog entry
- [ ] `db_config.version` is a real, publicly available DB release

### Methodology

- [ ] Ran at least 3 measurement runs after a discarded warm-up run
- [ ] Reported statistic and spread are disclosed in `DISCLOSURE.md`
- [ ] Host preparation steps from `docs/reproducibility.md` were applied
- [ ] No fairness-policy violations (test-set leakage, persistence
      disabled, cherry-picked best run reported as single, etc.)

### Vendor submissions only

- [ ] Filename uses the `*.vendor.json` suffix
- [ ] `DISCLOSURE.md` identifies the company and your role
- [ ] Confirmed the same fairness rules apply as to third-party submissions

---

## New adapter checklist

<!-- Use this section for new-adapter PRs. Delete otherwise.
     Required reading: docs/adapters.md. -->

- [ ] Linked the `New adapter` issue this PR resolves
- [ ] New crate `crates/vbench-<dbname>/` follows the layout of
      `crates/vbench-strata/`
- [ ] `BenchAdapter` impl present and complete
- [ ] Smoke test in `tests/smoke.rs` (`#[ignore]`'d if it requires a
      real DB binary; gated on an env var documented in the test file)
- [ ] Adapter fairness checklist from `docs/fairness.md#adapter-fairness-checklist`
      reviewed
- [ ] Cargo feature added in `crates/vbench-cli/Cargo.toml`
- [ ] Match arm added in `crates/vbench-cli/src/cmd_run.rs`
- [ ] `docs/adapters.md` and the README adapter table updated
- [ ] Smoke-test output against the real DB pasted below

```text
<paste smoke-test output here>
```

---

## Methodology / fairness change checklist

<!-- Use ONLY for changes to recall/NDCG formulas, units, fairness rules,
     or anything else in docs/methodology.md or docs/fairness.md. -->

- [ ] Discussion issue opened with the `policy-change` label
- [ ] At least 2 weeks of comment time elapsed
- [ ] Sign-off from at least 2 maintainers in the discussion
- [ ] `CHANGELOG.md` entry added describing the change and its
      effective date
- [ ] Schema-drift guard tests updated if the change affects the
      result schema
- [ ] Confirmed: this change does NOT retroactively affect existing
      published results (per `docs/fairness.md`)

---

## Reviewer notes

<!-- Optional. Anything you want a reviewer to look at first, known
     limitations, follow-up work, etc. -->
