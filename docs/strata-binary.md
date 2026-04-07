# How vbench drives the Strata binary

The Strata adapter is unusual: it never imports `strata-executor` source.
Instead it consumes the precompiled `strata` binary delivered via
`https://stratadb.org/install.sh` and drives it as a child process over
its built-in IPC daemon.

This document explains why and how.

## Why drive a binary?

Three reasons:

1. **Reproducibility.** A published benchmark number is only useful if
   the binary that produced it is downloadable. By depending on the
   released `strata` binary instead of a source revision, we can pin a
   `db_config.version` field in the result JSON that maps directly to
   a GitHub release tag.

2. **Install speed.** Linking `strata-executor` as a source dependency
   would mean every `cargo install vectordb-bench-rs` recompiles the
   entire strata-core workspace, which takes several minutes the first
   time. Installing the released binary is a 10 MB download.

3. **Honesty.** Strata is an embedded database, but its public surface
   is the CLI. Benchmarking the same surface a real user would touch
   gives a more representative number than benchmarking the internal
   library API directly.

## The lookup chain

`vbench-strata` finds the `strata` binary in this order:

1. **Explicit override**: the adapter's `params.strata_bin` field
   (resolved from the CLI's `--strata-bin <path>`)
2. **`STRATA_BIN` environment variable**
3. **`PATH` lookup** via the `which` crate
4. **`~/.strata/bin/strata`** — the default location used by
   `https://stratadb.org/install.sh`

If none of those resolve, the adapter returns a clear actionable error
pointing the user at install.sh:

```text
could not find a `strata` binary.
Install one with:
  curl -fsSL https://stratadb.org/install.sh | sh
…or set STRATA_BIN to an explicit path.
```

The lookup is in `crates/vbench-strata/src/locate.rs`.

## Subprocess lifecycle

`StrataAdapter::open` does the following at adapter-open time:

1. Locate the binary.
2. `tokio::process::Command::new(strata_bin).arg("up").arg("--fg").arg("--db").arg(workdir).kill_on_drop(true).spawn()`.
3. Poll for `<workdir>/strata.sock` with a 50 ms interval and a 10 s
   timeout. If it doesn't appear in time, kill the child and bail.
4. `StrataIpcClient::connect(&socket_path)`.
5. `client.ping()` → returns the server version, stored on the adapter
   for `AdapterInfo::db_version`.
6. `VectorCreateCollection { collection: "vbench", dimension, metric }`.

`shutdown(self)` is the inverse:

1. Drop the IPC client (closes the socket).
2. `child.kill().await` on the daemon.

`kill_on_drop(true)` is the belt-and-braces guarantee: even if the
runner panics between `open()` and `shutdown()`, the child gets a
SIGKILL when the `Child` value is dropped.

## CLI flag history

A heads-up for anyone reading the strata source: the `up` subcommand's
flag for foreground mode is `--fg`, not `--foreground`. The longer name
existed in earlier drafts of the CLI but was renamed before v0.6.0
shipped. The `vbench-strata-ipc` and `vbench-strata` smoke tests both
use the `--fg` form.

The workdir is also passed as `--db <PATH>`, not as a positional
argument. The full invocation is:

```bash
strata up --fg --db /tmp/some-workdir
```

## The wire protocol

Documented in detail in `crates/vbench-strata-ipc/src/types.rs`.
Summary:

- **Transport**: unix domain socket at `<workdir>/strata.sock`,
  owner-only readable
- **Framing**: 4-byte big-endian u32 length prefix + payload
- **Encoding**: MessagePack via `rmp_serde::to_vec_named` (named-field
  form, struct fields encoded as map keys)
- **Max frame**: 64 MB
- **Request**: `{ id: u64, command: Command }`
- **Response**: `{ id: u64, result: Result<Output, Error> }`

vbench-strata-ipc mirrors this format from `strata-core`'s
`crates/executor/src/ipc/wire.rs` and `protocol.rs`. The mirror is
intentionally **partial**: only the ~6 `Command` and ~6 `Output`
variants vbench actually needs.

## Mirrored Command variants

| Command | Why vbench needs it |
|---|---|
| `Ping` | handshake + version probe at `open()` time |
| `VectorCreateCollection` | provision the bench collection |
| `VectorBatchUpsert` | bulk-load training vectors |
| `VectorQuery` | k-NN search |
| `VectorDeleteCollection` | optional cleanup |
| `VectorCollectionStats` | reserved for Phase 2 (count round-trip) |

## Mirrored Output variants

| Output | Returned by |
|---|---|
| `Pong { version }` | `Ping` |
| `Version(u64)` | `VectorCreateCollection`, `VectorDeleteCollection` |
| `Versions(Vec<u64>)` | `VectorBatchUpsert` |
| `VectorMatches(Vec<VectorMatch>)` | `VectorQuery` |
| `Bool(bool)` | misc |
| `VectorCollectionList(Vec<rmpv::Value>)` | `VectorCollectionStats` (captured opaquely) |

## Opaque fields

Two fields are captured as `rmpv::Value` rather than mirrored:

1. **`metadata`** on `BatchVectorEntry` and `VectorMatch`. strata-core
   uses its own `Value` enum here, which is large and irrelevant to a
   vector benchmark. vbench always sends `None` and accepts whatever
   the server returns without inspecting it.

2. **`Error`** in the `Response::result` field. strata-core's `Error`
   enum has 50+ externally-tagged variants. We capture the whole error
   as `rmpv::Value` and provide a `Display` impl on `IpcError` that
   pretty-prints the `{VariantName: body}` form.

## Drift attribution

The `client.ping()` call at adapter-open time captures the strata
version into `AdapterInfo::db_version`, which the runner copies into
`task_config.db_config.version` of the published result. If a future
strata release changes the wire format, the version field is the
first place to look.

The CI smoke test (`crates/vbench-strata/tests/smoke.rs`) runs the
full open → load → count → optimize → search → shutdown flow against
a real strata daemon. It's `#[ignore]`'d by default; to run it locally:

```bash
STRATA_BIN=~/.strata/bin/strata cargo test -p vbench-strata -- --ignored
```

CI runs the cheap pure-serde round-trip tests in
`crates/vbench-strata-ipc/tests/round_trip.rs` (which catch struct-field
typos at compile time).

## Limitations

- **Single connection**: vbench-strata holds one `StrataIpcClient` in a
  `tokio::sync::Mutex`. Phase 2's concurrent QPS sweep will need to
  open multiple connections (or pool them) to actually hit Strata's
  parallelism.
- **No HNSW tuning**: strata 0.6.x doesn't expose `M`, `efConstruction`,
  or `efSearch` via the IPC protocol. The adapter records
  `db_case_config.metric_type` but cannot record HNSW parameters. When
  strata adds tuning to the IPC surface, vbench-strata will pick it up
  in the `params` schema.
- **`optimize()` is a no-op**: Strata's HNSW build is lazy and the
  runner's warm-up loop forces it via real queries. There's no separate
  "build index" RPC to call.

## Strata version compatibility

vbench-strata targets **strata 0.6.1 and later**. Earlier versions used
`--foreground` instead of `--fg` and a positional workdir argument.
strata 0.6.0 also lacked the bulk Arrow ingest fix and shipped without
the `arrow` and `cloud` features compiled in. Don't try to benchmark
against 0.6.0 — the numbers will be misleading.

The required minimum will rise over time. The version probe is the
canonical drift signal: if `client.ping()` returns a version older
than what vbench-strata was tested against, expect things to break.
