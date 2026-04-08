# Documentation roadmap

This document tracks the docs vbench still needs. It exists so:

- New contributors can claim a doc and write it without re-deriving
  what's missing
- Maintainers can decide which docs are blocking which features
- Anyone reading the repo can see what's planned vs what's shipped

If you want to write one of these, open an issue claiming it (or
just open a PR — the issue is courtesy, not a requirement).

## What's already shipped

For context, the current `docs/` baseline:

| File | Status |
|---|---|
| [`methodology.md`](methodology.md) | ✅ Phases, recall/NDCG formulas, units, defaults |
| [`datasets.md`](datasets.md) | ✅ Catalog, bundle layout, cache structure |
| [`adapters.md`](adapters.md) | ✅ `BenchAdapter` trait shape, lifecycle, contribution flow |
| [`interop.md`](interop.md) | ✅ Field-by-field upstream wire compat |
| [`strata-binary.md`](strata-binary.md) | ✅ How vbench drives the released strata binary |
| [`submitting-results.md`](submitting-results.md) | ✅ Submission process, two tracks, disclosure rules |
| [`fairness.md`](fairness.md) | ✅ What's allowed, what's cheating, enforcement |
| [`reproducibility.md`](reproducibility.md) | ✅ Recipe, host prep, statistical methodology |
| [`../results/HOST.md.template`](../results/HOST.md.template) | ✅ Standardised host snapshot format |

Plus the top-level [`README.md`](../README.md) and
[`CONTRIBUTING.md`](../CONTRIBUTING.md).

## Tier 3 — automation & maintenance

These keep the project healthy long-term. Most are top-level or
config files; only the ones marked **(doc)** belong under `docs/`.

### `docs/governance.md` **(doc)**

**What it should cover:**

- Who the current maintainers are (names + GitHub handles)
- How decisions get made for different change classes:
  - Trivial (typos, dep bumps): self-merge
  - Bug fixes: 1 reviewer
  - New features / adapters: 1 reviewer + a maintainer
  - Methodology / fairness changes: discussion period + 2 maintainer sign-off (per `fairness.md`)
  - Releases: 2 maintainer sign-off
- How to become a maintainer (consistent contribution + invitation)
- How a maintainer gets removed (inactivity / behaviour)
- Conflict-of-interest disclosure for maintainers (DB vendor employment)
- How disputes between maintainers get resolved

**Blockers:** the project needs more than one human maintainer for
most of this to be meaningful. Until then, governance is "the one
person who runs it makes the calls and documents them".

### `CHANGELOG.md` (top-level, not docs)

**What it should cover:** [Keep a Changelog](https://keepachangelog.com)
format. One entry per tagged release. Sections: `Added`, `Changed`,
`Fixed`, `Removed`, `Deprecated`, `Security`. Plus an "Unreleased"
section at the top.

**Blockers:** vbench has no tagged release yet (no `v0.1.0`). The
file should be created at the same time as the first release tag
so the first entry isn't a retroactive backfill.

### `CITATION.cff` (top-level, not docs)

**What it should cover:** [Citation File Format 1.2.0](https://citation-file-format.github.io)
metadata so academic papers using vbench numbers can cite the harness
properly. Fields: `title`, `authors`, `version`, `date-released`,
`url`, `repository-code`, `license`, `keywords`, `preferred-citation`.

**Blockers:** the same first-release blocker as `CHANGELOG.md`. The
`version` and `date-released` fields need a real release.

### `.github/dependabot.yml` (config, not docs)

**What it should cover:** weekly dep updates for the cargo workspace
and for GitHub Actions. Group patch updates so we don't get a flood
of trivial PRs. Schedule for off-hours.

**Blockers:** none. This is the easiest Tier 3 item to land.

### `deny.toml` + cargo-deny CI step (config, not docs)

**What it should cover:** `cargo-deny check` enforcing:

- Only allowed licenses (Apache-2.0, MIT, BSD-2/3, ISC, Unicode-3.0)
- No banned crates (e.g. ones with known maintenance issues)
- No security advisories from rustsec
- No multiple major versions of the same dependency where avoidable

Add a CI job that runs `cargo deny check` on every PR.

**Blockers:** none.

## Tier 4 — polish

These are nice-to-have, low-priority, but they're the kind of thing
that makes a benchmark project feel finished.

### `docs/glossary.md` **(doc)**

**What it should cover:**

- **recall@k** — what it measures, how vbench computes it (cross-link to
  methodology.md)
- **NDCG@k** — same, with the order-insensitivity caveat
- **HNSW** — Hierarchical Navigable Small World; explain `M`,
  `efConstruction`, `efSearch` and which knobs the leaderboard cares
  about
- **IVF** — Inverted File index; `nlist`, `nprobe`
- **PQ** — Product Quantization; codebooks, sub-vector dimensions
- **Cosine vs L2 vs IP** — which metric maps to which use case
- **Ground truth** — how the dataset's `neighbors.parquet` is computed
  upstream (typically brute-force exact search)
- **Test set leakage** — what it means and why the fairness policy
  bans it
- **QPS / p50 / p95 / p99** — percentile basics for readers new to
  latency analysis

**Blockers:** none. This is a pure writing task.

### MSRV documentation

**Where it lives:** a small section in `README.md` (and maybe
`CONTRIBUTING.md`'s development setup section), pointing at
`rust-toolchain.toml`. Not a separate doc file.

**What it should cover:** the pinned Rust version (currently 1.94.1),
how `rustup` auto-installs it on `cd` into the repo, what the upgrade
policy is (we follow stable; bumps land in their own PRs).

**Blockers:** none.

### Badges in README

**Where it lives:** the top of `README.md`, between the title and
the tagline.

**What it should include:**

- CI status (already there)
- License (already there)
- Latest release (once `v0.1.0` is tagged)
- crates.io version (if/when we publish)
- MSRV badge (`https://img.shields.io/badge/rustc-1.94+-blue.svg`)

**Blockers:** the latest-release badge needs a real release.

### Pre-commit hooks

**Where it lives:** a small section in `CONTRIBUTING.md` plus an
optional `.pre-commit-config.yaml` or a hand-rolled `.git/hooks/pre-commit`.

**What it should run:** `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets -- -D warnings`. Skip the
test run (too slow for every commit).

**Blockers:** none. Useful for active contributors but not strictly
required.

### `xtask` shortcuts (config, not docs)

**Where it lives:** a new `xtask/` crate following the
[xtask convention](https://github.com/matklad/cargo-xtask).

**What it should do:** wrap the common workflows so contributors
type `cargo xtask test-all` instead of remembering the exact flag
combinations:

- `cargo xtask check` → fmt + clippy + test
- `cargo xtask live-test` → run the `STRATA_BIN`-gated tests
- `cargo xtask publish-result` → package a result directory
- `cargo xtask verify <result.json>` → re-run the same config and
  diff against the published numbers

**Blockers:** none. Mostly bikeshedding what to call the targets.

## Beyond the tiers

Docs that aren't part of any tier but will be needed eventually.

### `docs/leaderboard.md`

**What it should cover:** how the `results/` tree is rendered into
a human-readable leaderboard. Sort order, grouping by case_id,
verification badges, vendor-tagged entries, link out to the source
JSON. Includes the contract between the rendering tool and the
result file format.

**Blockers:** the leaderboard renderer doesn't exist yet. This doc
is written when the renderer is. Until then, the `results/` tree IS
the leaderboard and humans read it directly.

### `docs/release-process.md`

**What it should cover:** how vbench itself is versioned and released.
The release workflow lives in `.github/workflows/release.yml`, but
we need a doc that explains:

- Versioning policy (semver: breaking schema → major; new fields → minor;
  bug fixes → patch)
- When to bump
- The release checklist (CHANGELOG entry, version bump in
  workspace Cargo.toml, tag, watch the workflow, smoke-test the
  release binary)
- How to yank a bad release

**Blockers:** the policy needs to be decided once we have an actual
v0.1.0. Drafting this in advance is fine but it'll need an update
after the first release.

### `docs/troubleshooting.md`

**What it should cover:** common errors users hit, with the fix:

- "could not find a `strata` binary" → install via `install.sh` or set `STRATA_BIN`
- "strata.sock did not appear within 10s" → daemon failed to start, check `--db` workdir permissions
- "post-load count mismatch" → adapter bug or partial load; check the daemon logs
- "DistanceMetric mismatch" / dim mismatch errors
- Cohere-1M download failures (Zilliz CDN intermittent issues)
- Out-of-memory loading the train set (3 GB resident; check RAM)
- Very high p99 → host preparation forgotten (cpufreq governor, drop caches, other workload)

**Blockers:** need real user reports to know what the common issues
actually are. Bootstrap with the issues that came up during Phase 1
development; add new entries as the bug tracker fills in.

### `docs/upgrading.md`

**What it should cover:** migration notes between vbench versions
when they introduce breaking changes — schema evolution, CLI flag
renames, dropped adapters, etc.

**Blockers:** no breaking changes have happened yet (pre-`v0.1.0`).
Created on first breaking change.

### `docs/wire-protocol.md`

**What it should cover:** broader IPC details that don't fit in
`strata-binary.md`. The MessagePack frame format, the error model,
the version-pinning strategy across the strata-core / vbench-strata-ipc
boundary, what changes are wire-compatible vs breaking.

**Blockers:** `strata-binary.md` already covers most of this. This
doc only needs to exist if/when a second adapter (Qdrant, Lance) uses
a different wire protocol and we need a meta-doc covering both.
Probably write it during the Phase 2 Qdrant adapter work.

## Out of scope (not docs)

The following are tracked but aren't documentation tasks. Listed
here so the roadmap is exhaustive:

- **First published Cohere-1M result** — depends on running the
  benchmark on a controlled host. Will produce
  `results/2026-04/strata-cohere-1m/` with the four required artifacts
  per `docs/submitting-results.md`. Tracked separately.
- **Phase 2: concurrent QPS sweep** — code, not docs.
  [Issue #5](https://github.com/stratalab/vectordb-bench-rs/issues/5).
- **Phase 2: Qdrant adapter** — code. Will trigger the
  `docs/wire-protocol.md` write-up.
- **Leaderboard renderer** — code. Will trigger
  `docs/leaderboard.md`.

## How to claim a doc

Open an issue with the `docs` label saying which one you're
writing. Or skip the issue and open the PR directly — we won't
turn down a useful doc PR for missing the courtesy issue.

Acceptance criteria for any doc PR:

- Cross-references to existing docs use relative paths and resolve
  on GitHub
- The fairness, methodology, and submission docs are treated as
  load-bearing — don't contradict them; if you find a problem,
  open a `policy-change` discussion issue first
- Markdown is plain CommonMark; no GitHub-only flavoured features
  unless they degrade gracefully
- Examples (commands, code snippets) are copy-pasteable and tested
  on at least one platform

## Order of operations (suggested)

If we did this strictly in order of value:

1. **`.github/dependabot.yml`** — easiest, biggest long-term win
2. **`docs/glossary.md`** — pure writing, helps every newcomer
3. **`docs/troubleshooting.md`** — high user value, can be bootstrapped from existing knowledge
4. **`deny.toml` + cargo-deny CI** — security hygiene
5. **First Cohere-1M result** — unblocks `CHANGELOG.md`, `CITATION.cff`, `v0.1.0`, badges
6. **`CHANGELOG.md`** + **`CITATION.cff`** + tag `v0.1.0`
7. **`docs/governance.md`** — once there are real maintainers
8. **`docs/release-process.md`** — once we've done one release
9. **`xtask/`** — quality-of-life for contributors

Tier 3 items 1–4 can land in any order. Item 5 is the gating event
for everything in 6 onward.
