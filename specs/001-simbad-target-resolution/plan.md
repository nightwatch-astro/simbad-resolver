# Implementation Plan: SIMBAD Target Resolution (simbad-resolver)

**Branch**: `001-simbad-target-resolution` | **Date**: 2026-07-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-simbad-target-resolution/spec.md`

## Summary

Build `simbad-resolver`, an embeddable Rust library that resolves an astronomical name/designation or a sky position to a canonical target identity against SIMBAD, backed by a pluggable cache. The design is a **granular `simbad-resolver-*` workspace** (main installable crate `simbad-resolver`) with a pure/sync core, two first-class resolver backends (TAP + Sesame), pluggable `Cache` and `Queue` abstractions each shipped in an in-memory and a durable (SQLite) form, and a facade that provides cache-first resolve, sticky user-override precedence, and an async batch resolver. The resolution logic (2-round-trip TAP querying, normalization, dedup-by-physical-id, precedence, Caldwell translation, object-type mapping, defensive response handling) is carried over from the proven astro-plan implementation; the work is decoupling (configurable identity/etiquette, owned schema, pluggable backends), simplification (drop hand-rolled workarounds now that dependencies are unconstrained), and additions (Sesame, cone search, async batch).

## Technical Context

**Language/Version**: Rust 2021, MSRV pinned at a recent stable (`rust-version = 1.82`), revisited as needed.

**Primary Dependencies** (dependencies are unconstrained — best crate per job; library public errors are typed via `thiserror`, `anyhow` only in the seed-builder bin / examples / tests):
- Core (pure): `serde`/`serde_json`, `thiserror`, `uuid` (v4+v5), `unicode-normalization`, `async-trait`.
- HTTP: `reqwest` (rustls) — free to use its query builder / json helpers rather than hand-rolling percent-encoding; `url` for endpoint validation.
- Wire parsing: `csv` (SIMBAD TSV `basic` rows).
- Cache/queue backends: `dashmap` (in-memory); `sqlx` (sqlite + rustls + migrate) for the durable backend (owns its migrations); `redb` is a documented future pure-Rust alternative.
- Runtime/obs: `tokio`; `time` (RFC3339 timestamps — `now_iso` inlined, **no** `domain_core` dependency); `tracing`.
- **Dropped** astro-plan workarounds: manual ADQL percent-encoding, hand-rolled https endpoint check, the `domain_core` dependency, and dead deps (`strsim`).

**Storage**: pluggable. In-memory (`dashmap`) and durable local SQLite (`sqlx`, crate-owned migrations for `canonical_target`, `target_alias`, `pending_resolution`). No `resolver_settings` table — settings are caller-owned config. Callers may supply their own `Cache`/`Queue` implementations.

**Testing**: `cargo test` (unit + integration). Network is behind the `Resolver` trait so unit/orchestration tests run offline with `FakeResolver`. Live SIMBAD tests are `#[ignore]`-gated (`just test-live`). Record/replay fixtures cover the TAP/Sesame parsers so SC-002-adjacent behavior is exercised in CI without live network. One cross-backend behavior suite runs against both the memory and SQLite `Cache` (SC-006).

**Target Platform**: any Rust target hosting `tokio` + `reqwest`(rustls) + `sqlx`(sqlite); the pure `-core` crate builds anywhere with no async/net/db (SC-005). Embeddable library, not a service.

**Project Type**: Cargo workspace of libraries (+ one maintainer bin for a future seed package). Main installable crate: `simbad-resolver`.

**Performance Goals**: typeahead search over a populated cache < 100 ms with no network (SC-001); resolve-at-most-once per physical object (SC-002); batch resolver honours a bounded concurrency and is polite to CDS.

**Constraints**: never fabricate coordinates/identity (typed unresolved outcomes); offline-capable (cache/seed keep working with no network); bounded/defensive HTTP (body-size cap, error-body detection, id normalization); exact-normalized matching only (no fuzzy resolution).

**Scale/Scope**: unbounded upstream catalog (any SIMBAD object); caches from a handful to ~10^5 entries; alias index sized to support sub-100 ms prefix/substring search at that scale.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

The project constitution is still the unfilled template, so there are no ratified gates to evaluate; the check is **informational**. The library nonetheless commits to these de-facto principles, which double as design gates and are all satisfied by this plan:

- **Never fabricate** — resolution returns typed unresolved outcomes, never invented coordinates. ✅
- **Pure core** — identity/normalization/types compile with zero async/net/db deps (SC-005). ✅
- **Pluggable boundaries** — resolution and storage are trait-level seams (`Resolver`, `Cache`, `Queue`); backends are substitutable (SC-004/006). ✅
- **Decoupled/configurable** — no hardcoded identity namespace, endpoint, or User-Agent; owned schema; no app-specific coupling. ✅
- **Deliberate scope** — no bundled seed, no UI, no local FOV geometry (that is `target-match`). ✅

*(A ratified `constitution.md` can be authored later via `/speckit.constitution`; nothing here would violate the principles above.)*

## Project Structure

### Documentation (this feature)

```text
specs/001-simbad-target-resolution/
├── plan.md              # This file
├── research.md          # Phase 0 — decisions + rationale
├── data-model.md        # Phase 1 — entities + SQLite schema
├── quickstart.md        # Phase 1 — runnable validation scenarios
├── contracts/           # Phase 1 — public API (traits + types)
│   ├── resolver.md      # Resolver trait, ResolvedIdentity, ResolveError, resolve-by-position
│   ├── cache.md         # Cache trait
│   ├── queue.md         # Queue trait + batch resolver
│   └── facade.md        # simbad-resolver orchestration API + config
├── checklists/
│   └── requirements.md  # Spec quality checklist (from /speckit.specify)
└── tasks.md             # Phase 2 — /speckit.tasks (not created here)
```

### Source Code (repository root)

```text
Cargo.toml                       # workspace root (members below; shared deps/lints)
crates/
├── simbad-resolver-core/        # pure & sync: types, Resolver trait, normalize, identity, wire helpers, Offline/Fake
│   ├── src/{lib,object_type,source,identity as ident,normalize,resolver,error,wire}.rs
│   └── tests/
├── simbad-resolver-caldwell/    # static C1–C109 → designation map
├── simbad-resolver-tap/         # SimbadTapResolver: name resolve (2 round-trips) + cone search
├── simbad-resolver-sesame/      # SimbadSesameResolver: name resolve; optional enrichment via supplied resolver
├── simbad-resolver-cache/       # Cache trait (+ Queue trait) — interface only
├── simbad-resolver-cache-memory/# dashmap Cache + in-memory Queue
├── simbad-resolver-cache-sqlite/# sqlx Cache + durable cache-backed Queue; owns migrations/
│   └── migrations/0001_init.sql # canonical_target, target_alias, pending_resolution
└── simbad-resolver/             # MAIN facade + orchestration + async batch; re-exports the ecosystem
    ├── src/{lib,config,orchestrate,batch}.rs
    └── tests/                    # cross-backend suite (memory ↔ sqlite), degrade/override, batch
```

**Structure Decision**: A granular Cargo workspace under `crates/`. The main installable crate is `simbad-resolver`, which re-exports `-core` and the trait crates and, via feature flags, the concrete resolver/cache backends. The `Resolver`, `Cache`, and `Queue` traits are **async** (`async-trait`, dyn-compatible) because the durable backends (`reqwest`, `sqlx`) are async; in-memory impls satisfy the async traits trivially. The `Queue` trait lives in `simbad-resolver-cache` alongside `Cache`; the durable queue impl shares the SQLite database with the durable cache. See [research.md](./research.md) for the split rationale and [data-model.md](./data-model.md) for the schema.

## Complexity Tracking

The eight-crate split is more granular than a single crate; it is justified by hard requirements, not preference:

| Choice | Why needed | Simpler alternative rejected because |
|--------|-----------|--------------------------------------|
| Separate pure `-core` crate | SC-005: type/normalize-only consumers must pull zero async/net/db deps | A single crate (even feature-gated) still resolves the full dependency tree for pure consumers and complicates feature unification |
| `Cache`/`Queue` as their own trait crate + separate `-memory`/`-sqlite` impl crates | SC-006 + user directive: backends must be swappable without touching orchestration, and the heavy `sqlx` surface must be opt-in | Bundling impls into one crate forces every consumer to compile `sqlx`+`dashmap` even when they only want one (or a custom) backend |
| `-tap` and `-sesame` as co-equal peer crates | Both are first-class, independently selectable; Sesame must not hard-depend on TAP | Folding Sesame into `-tap` (or gating by feature) couples two independent network backends and their differing dependency/parse surfaces |
| `-caldwell` as its own crate | It is catalog data, optional, and independently versionable | Baking it into `-core` forces the Caldwell table on consumers who don't want it and blurs the "pure primitives" boundary |
