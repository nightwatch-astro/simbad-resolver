# Research: SIMBAD Target Resolution

Phase 0 decisions. Format: **Decision → Rationale → Alternatives considered**. Much of the resolution behavior is carried over from the proven astro-plan `targeting_resolver`; this document records the decisions that differ or that a fresh reader needs.

## R1 — Crate granularity (8-crate `simbad-resolver-*` workspace)

**Decision**: Split into `-core`, `-caldwell`, `-tap`, `-sesame`, `-cache`, `-cache-memory`, `-cache-sqlite`, and the main `simbad-resolver` facade.

**Rationale**: SC-005 requires a pure/sync core with zero async/net/db deps; SC-006 + the user directive require swappable cache/queue backends with the heavy `sqlx` surface opt-in; TAP and Sesame are co-equal first-class resolvers. Trait crates + per-impl crates express exactly these seams.

**Alternatives**: (a) single crate with feature flags — rejected: pure consumers still resolve the full dep tree and feature unification is fragile. (b) core + one "resolver" crate — rejected: bundles TAP+Sesame+sqlx+dashmap, defeating the opt-in weight goal.

## R2 — Async `Resolver` / `Cache` / `Queue` traits (dyn-compatible)

**Decision**: All three seams are `#[async_trait]` traits, object-safe (`dyn Resolver`, `dyn Cache`, `dyn Queue`).

**Rationale**: durable backends are async (`reqwest`, `sqlx`); an async trait lets the in-memory impls satisfy it trivially while the SQLite impl avoids `block_on` in an async context. Object-safety lets the facade hold `Arc<dyn …>` and swap backends at runtime.

**Alternatives**: sync traits with `spawn_blocking` in the SQLite impl — rejected (awkward, error-prone); native AFIT without `async-trait` — deferred (dyn-compat still easiest via `async-trait` today).

## R3 — Dependencies are unconstrained; drop astro-plan's hand-rolled workarounds

**Decision**: Use the best crate per job. Library public errors are typed (`thiserror`); `anyhow` only in the maintainer bin / examples / tests. Specifically drop: manual ADQL percent-encoding (use `reqwest`/`url`), hand-rolled https endpoint validation (use `url`), the `domain_core` dependency (inline the 3-line RFC3339 `now`), and dead deps (`strsim`).

**Rationale**: astro-plan minimized deps for a Tauri desktop bundle; this library has no such constraint. Removing the workarounds reduces code and bug surface.

**Alternatives**: keep the workarounds for byte-fidelity with the origin — rejected: no value here, and they were flagged as friction in the origin.

## R4 — TAP resolution: two ADQL round-trips + defensive parsing (carried over)

**Decision**: Resolve-by-name via (1) `basic ⋈ ident` to find distinct physical object(s) (oid, main_id, ra, dec, otype), (2) `ident` for the winning oid to pull the full alias + `NAME …` common-name set. 0 rows → not-found; >1 distinct oid → ambiguous. Add resolve-by-position via an ADQL cone search (`CONTAINS(POINT, CIRCLE)`) returning nearest object(s) + separation. Keep the defensive parsing: bound body size, detect VOTable/HTTP-200 error bodies, strip the TSV header, unquote and collapse whitespace-padded ids, range-validate RA∈[0,360)/Dec∈[-90,90] (out-of-range → not-found).

**Rationale**: proven live against SIMBAD; precise and structured; ambiguity is detectable. Cone search is the natural SIMBAD-side position query.

**Alternatives**: single-shot name→coord — rejected: loses the full alias set and ambiguity detection.

## R5 — Sesame resolver: standalone with optional enrichment

**Decision**: `SimbadSesameResolver` resolves via the Sesame service (aggregating SIMBAD/NED/VizieR) and returns whatever Sesame gives, which may be coarser (missing otype / full alias set). It accepts an **optional** caller-supplied resolver to enrich a hit (e.g. re-resolve the returned main-id via TAP), but has **no build dependency** on `-tap`.

**Rationale**: broader name coverage as a peer backend without coupling the two crates; the consumer decides whether to pay for enrichment. (Clarification 2026-07-11.)

**Alternatives**: always-enrich (hard dep on `-tap`) — rejected: couples the crates and forces two network calls; coarse-only with no enrichment path — rejected: too lossy for callers who need type/aliases.

## R6 — Identity: dedup by physical id; deterministic UUIDv5 with configurable namespace

**Decision**: The dedup key is the SIMBAD physical-object id (`oid`); when unknown (seed/override-only), fall back to `UUIDv5(namespace, primary_designation)`. The namespace is a **caller-supplied** value (an existing consumer passes their historical namespace, e.g. `"astro-plan.targets"`, for id continuity; a fresh consumer picks their own).

**Rationale**: oid is the true physical identity (collapses aliases/catalogs onto one target, FR-005); the configurable namespace removes the astro-plan-specific hardcode while preserving drop-in id stability (SC-007).

**Alternatives**: designation-only identity — rejected: splits one object across catalogs; hardcoded namespace — rejected: breaks reuse/continuity.

## R7 — Source precedence with sticky override (carried over)

**Decision**: `seed < resolved < user-override`. A write may overwrite an existing row iff its source precedence ≥ the existing row's; a `user-override` is therefore sticky against later `resolved`/`seed` writes. `upsert` dedups by oid (or derived id) and rewrites aliases wholesale.

**Rationale**: durable human corrections must survive automation (FR-006/FR-007); minimal, generalizable model.

**Alternatives**: last-write-wins — rejected: clobbers overrides.

## R8 — Storage: pluggable `Cache`/`Queue`; ship memory + SQLite; `redb` future

**Decision**: `Cache` and `Queue` traits with two shipped backends each — in-memory (`dashmap`) and durable SQLite (`sqlx`, crate-owned migrations). The durable `Queue` is **cache-backed**: it persists pending items in a `pending_resolution` table in the same SQLite database, surviving restarts. No general-purpose eviction cache (`moka`/`lru`) is used — this is a durable dedup/typeahead store, not a TTL cache. `redb` (pure-Rust, no C dependency) is a documented future durable backend behind the same traits.

**Rationale**: SC-006 substitutability; SQLite is the natural embeddable durable default with migrations + indexed search; `dashmap` gives lock-free in-memory concurrency; the clarification asked for a cache-backed pluggable queue plus both memory and durable backends.

**Alternatives**: `moka`/`lru` as the cache — rejected: eviction semantics are wrong for durable dedup; `rusqlite` (sync) — rejected: needs `spawn_blocking` under async; single fixed backend — rejected: excludes valid deployments.

## R9 — Settings are caller-owned config (no persisted table)

**Decision**: online-enabled, endpoint, request timeout, and User-Agent are constructor configuration; the library persists none of them, and the durable schema has no `resolver_settings` table.

**Rationale**: a library should not own user preferences; the app that embeds it does. (Clarification 2026-07-11.) Simplifies the schema.

**Alternatives**: persisted singleton (astro-plan) — rejected as app-specific.

## R10 — Object-type taxonomy + raw escape hatch (carried over, extended)

**Decision**: Map SIMBAD `otype` → a closed 12-variant enum via a total function (unknown/empty → `Other`); additionally retain the **raw** otype string on the resolved identity so consumers wanting finer types are not limited by the closed set.

**Rationale**: the closed set is proven, load-bearing domain knowledge; the raw string is a cheap, forward-compatible extension for richer consumers.

**Alternatives**: closed-only — rejected: loses information some consumers need; open string-only — rejected: loses the ergonomic closed classification.

## R11 — Coordinates: decimal-degree seam now, `astro-angle` later

**Decision**: Expose ICRS J2000 decimal degrees (`f64` RA/Dec) with a conversion seam; adopt `astro_angle::Equatorial` as the public coordinate type once `astro-angle` exists. Sexagesimal parsing/validation for coordinate-input paths (cone-search input) uses `astro-angle` when available; a minimal inline parse until then.

**Rationale**: `astro-angle` doesn't exist yet; a seam keeps v1 unblocked and the migration forward-compatible. (User directive.)

**Alternatives**: block on `astro-angle` — rejected: unnecessary coupling to an unbuilt crate; never adopt it — rejected: loses shared-type interop with `target-match`.

## R12 — Ecosystem boundary (documented, not implemented here)

**Decision**: `simbad-resolver` is upstream (name → identity). It does **not** implement local nearest-neighbour / FOV-optics geometry — that is the downstream `target-match` crate (formerly `astro-target-id`), which consumes this library's output. Our cone search is a complementary SIMBAD-side query. Shared coordinate types come from `astro-angle`.

**Rationale**: fixed upstream→downstream flow keeps responsibilities clean (FR-020).

**Alternatives**: fold `target-match` in — rejected: opposite data flow, forces pure geometry under a network/db crate.

## R13 — Testing strategy

**Decision**: Unit + orchestration tests run offline via `FakeResolver` and the in-memory `Cache`/`Queue`. TAP/Sesame parsers are covered by **record/replay fixtures** (captured SIMBAD responses) so parse/dedup/ambiguity/error paths run in CI with no network. Live SIMBAD tests are `#[ignore]`-gated (`just test-live`). One cross-backend behavior suite runs unchanged against the memory and SQLite caches (SC-006).

**Rationale**: fast, deterministic CI; live path still verifiable on demand; SC-004/006 explicitly exercised.

**Alternatives**: live-only tests — rejected: flaky, network-bound, impolite to CDS.
