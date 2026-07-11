# Quickstart / Validation: SIMBAD Target Resolution

Runnable scenarios that prove the feature works. Detailed types live in
[data-model.md](./data-model.md) and [contracts/](./contracts/).

## Prerequisites

- Rust 2021 toolchain (`rustup`, stable ≥ 1.82), `just`.
- No network needed except the live scenario (S5).

## Build & test

```bash
just build          # cargo build --workspace --all-features
just test           # offline unit + integration + cross-backend suite
just lint           # fmt --check + clippy -D warnings
just test-live      # #[ignore]-gated live SIMBAD tests (network)
```

## Scenarios

### S1 — Resolve a name (US1 / FR-001, offline via FakeResolver)

Given a `FakeResolver` seeded with M 31, resolving `M31`, `m 31`, and `NGC 224`
all return **one** identity with `object_type = galaxy`, valid ICRS coords, and
aliases including `NGC 224` + `Andromeda Galaxy`. Resolving `zzz-not-a-thing`
returns `Unresolved{Unknown}` with **no** coordinate. → `simbad-resolver/tests/resolve.rs`.

### S2 — Cache-first + offline typeahead (US2 / FR-003, FR-010, SC-001/004)

Populate a cache (memory or sqlite), disconnect: `search("androm", 20)` returns
M 31 ranked, in < 100 ms; a second `resolve("M 31")` performs **zero** resolver
calls (assert `FakeResolver::call_count` unchanged). → cross-backend suite.

### S3 — Sticky override (US5 / FR-006/007)

`apply_override(target, "My M31")` then `resolve("M 31")` online returning a
different result → the override is retained (`source = user-override`). → `tests/override.rs`.

### S4 — Batch resolve with retry semantics (US4 / FR-011)

Enqueue `{cached, resolvable, unknown, transient-fail}` by opaque id; `drain()`:
cached inline, resolvable → resolved once, unknown → unresolved (attempts=1),
transient → still pending (attempts=0). Runs against in-memory **and** durable
(cache-backed) `Queue`. → `tests/batch.rs`.

### S5 — Live SIMBAD (SC-002, `--ignored`)

`cargo test -p simbad-resolver-tap --test live -- --ignored` resolves `M 31`,
`NGC 7293`, and a Caldwell (`C 14`) against real SIMBAD; asserts real coords,
type mapping, and the Caldwell→NGC alias binding. Also a cone search around
M 31's position returns M 31 nearest.

### S6 — Backend substitutability (SC-006)

The `cache_behavior` suite is generic over `impl Cache`; it is instantiated once
for `MemoryCache` and once for `SqliteCache` (in-memory sqlite) and must pass
identically. → `simbad-resolver-cache/tests/behavior.rs` (or a shared harness).

### S7 — Pure core, zero heavy deps (SC-005)

`cargo tree -p simbad-resolver-core -e normal` shows no `tokio`/`reqwest`/`sqlx`.
`simbad-resolver-core` builds and its unit tests pass with no async runtime.

### S8 — Configurable identity (SC-007)

`target_id_from_designation(&namespace("astro-plan.targets"), "M 31")` equals the
reference UUIDv5 value, proving drop-in id continuity for an existing consumer.
