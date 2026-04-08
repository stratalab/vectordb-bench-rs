# Contributing to vectordb-bench-rs

Thanks for being interested in vbench. This document covers the
mechanics of getting a change in. The substance — methodology,
fairness, the trait shape — lives under [`docs/`](docs/), and the
high-level pitch is in the [README](README.md).

## What kind of contribution?

Three contribution paths cover almost everything:

| Path | What | Where to start |
|---|---|---|
| **Submit a result** | Run vbench against a DB on your hardware and publish the numbers | [`docs/submitting-results.md`](docs/submitting-results.md) |
| **Add an adapter** | Wire a new vector database into vbench so it can be benchmarked | [`docs/adapters.md`](docs/adapters.md) |
| **Verify a result** | Reproduce someone else's result on a similar host and publish your numbers as a verification | [`docs/reproducibility.md`](docs/reproducibility.md) |

Beyond those: bug fixes, doc improvements, new datasets, methodology
proposals, and harness features are all welcome. See the
[issue templates](.github/ISSUE_TEMPLATE/) for the right entry point.

## Before you start

For non-trivial changes, **open an issue first**. This includes:

- New adapters (use the `New adapter` issue template)
- New datasets (`New dataset` template)
- Methodology / fairness policy changes
- Harness features that touch the runner, the result schema, or the
  CLI surface
- Disputes about a published result (`Result dispute` template)

You don't need an issue first for: typos, doc clarifications, dep
bumps that pass CI, formatting fixes, obvious bug fixes with a clear
root cause.

## Development setup

Requirements:

- Rust toolchain matching `rust-toolchain.toml` (currently 1.94.1).
  `rustup` will install it automatically when you enter the repo.
- A Unix-like OS. Windows is not on the release matrix and the
  `vbench-strata` adapter uses unix-domain sockets via tokio.
- For live tests against the Strata adapter: a `strata` binary
  installed via `curl -fsSL https://stratadb.org/install.sh | sh`,
  exposed via the `STRATA_BIN` environment variable.

```bash
git clone https://github.com/stratalab/vectordb-bench-rs
cd vectordb-bench-rs
cargo build --workspace
cargo test --workspace
```

## Local development loop

Before you push, run all three:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three on every PR. Save yourself a round-trip and
run them locally first.

To run the live tests against a real Strata daemon:

```bash
STRATA_BIN=~/.strata/bin/strata cargo test --workspace -- --ignored
```

The live tests are gated behind `STRATA_BIN` so CI doesn't try to run
them. They're the canary that catches wire-protocol drift between the
`vbench-strata-ipc` mirror and the upstream Strata IPC server.

## Branches and commits

Branch names use a type prefix:

| Prefix | Use for |
|---|---|
| `feat/` | New functionality |
| `fix/` | Bug fixes |
| `docs/` | Documentation only |
| `chore/` | Build, deps, CI, formatting |
| `refactor/` | Internal restructure with no behaviour change |
| `test/` | Tests only |
| `release/` | Version bumps and release prep |

Examples: `feat/qdrant-adapter`, `fix/recall-divide-by-zero`,
`docs/strata-binary-protocol`.

Commit messages use the same prefixes:

```
feat: vbench-strata adapter — drives released strata binary via IPC

Implements BenchAdapter for the Strata embedded vector database.
[…]

Co-Authored-By: Your Name <your@email>
```

The first line is a one-sentence summary. The body explains *why*,
not just *what*. Long-form context is welcome — the schema-fix and
release-fix commits are good examples.

## Pull requests

Open the PR against `main`. The PR template will prompt you for the
relevant checklist. **Don't skip the checklist** — it's the same
checklist a reviewer will run through, so completing it up front
makes the review faster.

### What CI runs

Three jobs:

1. `rustfmt` — `cargo fmt --all -- --check`
2. `clippy` — `cargo clippy --workspace --all-targets -- -D warnings`
3. `test` — `cargo test --workspace`

All three must be green before merge. Auto-merge with `--auto` is the
preferred merge mechanism (queues the merge until CI passes).

### Reviews

For non-trivial changes: at least one maintainer review. For tiny
fixes (typos, dep bumps, fmt): self-merge with admin is acceptable
if CI is green.

Reviewers will look at:

- Tests for new code paths
- Doc updates if you changed a public API or a documented behaviour
- Schema-drift implications if you touched `result.rs` or
  `metrics.rs`
- Fairness implications if you touched a `BenchAdapter` impl
- Backwards compatibility if you touched the wire protocol mirror

If you can flag these proactively in the PR description, the review
gets faster.

### Squash merge

We squash on merge. The PR title becomes the commit subject; the
body becomes the commit body. Write the PR title carefully — it's
what shows up in `git log`.

## Adding a new adapter

Step-by-step in [`docs/adapters.md`](docs/adapters.md). The short
version:

1. Open a `New adapter` issue describing the DB and how it'll be
   wired (library / server / binary subprocess)
2. Create `crates/vbench-<dbname>/` mirroring the layout of
   `crates/vbench-strata/`
3. Implement `BenchAdapter` for the new DB
4. Add a smoke test in `tests/smoke.rs` (gated on an env var if it
   needs the real DB)
5. Wire a Cargo feature in `crates/vbench-cli/Cargo.toml` and add a
   match arm in `cmd_run.rs`
6. Run fmt + clippy + test
7. Open the PR; the smoke test output goes in the description

## Adding a new dataset

Step-by-step in [`docs/datasets.md`](docs/datasets.md). The short
version:

1. Open a `New dataset` issue with the dataset's metadata
2. Add an entry to `CATALOG` in `crates/vbench-core/src/dataset.rs`
3. Set `case_id` to the matching upstream `CaseType` enum value
4. Confirm the bundle is hosted on a stable HTTPS endpoint
5. Add a small unit test that asserts the new id resolves
6. Open the PR

## Submitting a benchmark result

The most valuable contribution. See
[`docs/submitting-results.md`](docs/submitting-results.md) for the
process. Short version:

1. Run vbench against the DB on a controlled host (see
   [`docs/reproducibility.md`](docs/reproducibility.md) for host
   prep)
2. Use the [HOST.md template](results/HOST.md.template) to record
   the host snapshot
3. Open a PR adding the four required artifacts under
   `results/<YYYY-MM>/<adapter>-<dataset>/`
4. The PR template's "Result submission" section walks you through
   the disclosure questions

Vendor-submitted results are allowed and tagged via the
`*.vendor.json` filename suffix.

## Methodology and fairness

Changes to the recall/NDCG formulas, the unit conventions, or the
fairness policy go through a longer process:

1. Open a `Methodology / fairness change` discussion issue
2. Two-week comment period
3. Sign-off from at least 2 maintainers
4. `CHANGELOG.md` entry
5. PR with the implementation

We don't change methodology retroactively. A result that was fair
under the policy at the time of submission stays fair forever. See
[`docs/fairness.md`](docs/fairness.md#updates-to-this-policy) for
the rationale.

## Tone

Be kind, be specific, focus on the code or the methodology, not the
person. Maintainers will enforce.

## Security

Vulnerabilities in vbench itself (path traversal in the cache,
crafted dataset files that crash the parquet decoder, schema
injection that bypasses fairness checks, etc.) should be reported
**privately**, not via the public issue tracker — open a [GitHub
private security advisory](https://github.com/stratalab/vectordb-bench-rs/security/advisories/new)
on this repo.

Vulnerabilities in the *databases vbench benchmarks* should be
reported to the relevant vendor, not to us.

## License

By contributing, you agree that your contribution will be licensed
under the same [Apache-2.0](LICENSE) terms as the rest of the project.

## Where to ask

- **Bug?** Open an issue with the `Bug` template.
- **Feature idea?** Open a discussion issue first if it's non-trivial.
- **Adapter question?** [`docs/adapters.md`](docs/adapters.md), then
  the `New adapter` issue template.
- **Methodology question?** Read [`docs/methodology.md`](docs/methodology.md)
  and [`docs/fairness.md`](docs/fairness.md), then open a discussion
  issue.
- **Result submission question?** Read
  [`docs/submitting-results.md`](docs/submitting-results.md), then
  open an issue if anything's unclear.

Welcome aboard.
