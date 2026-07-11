# ADR 0001 — Stack and crate architecture

- Status: accepted
- Date: 2026-07-11
- Deciders: Sjors Robroek

## Context

`simbad-resolver` is extracted and generalized from the target-resolution
subsystem of an astronomy imaging app (`astro-plan`, crates `targeting` +
`targeting_resolver`). The goal is a standalone, embeddable library that
resolves astronomical names/positions to canonical identities via SIMBAD,
decoupled from the origin app's database schema, UUID namespace, HTTP branding,
and product features (ingest pipeline, UI, settings panes).

## Decisions

### Language / toolchain

- **Rust 2021**, edition- and dependency-pinned at the workspace level. MSRV
  targets a recent stable (`rust-version = "1.82"`), revisited as needed.
- `cargo fmt` + `cargo clippy -D warnings` enforced in CI and pre-commit.

### Crate split (granular)

The main installable crate is `simbad-resolver`. Supporting crates share the
`simbad-resolver-*` namespace (the `axum` / `tower-http` pattern):

| Crate | Responsibility |
|---|---|
| `simbad-resolver-core` | pure types, normalization, identity, `Resolver` trait |
| `simbad-resolver-tap` | SIMBAD TAP client (name resolve + cone-search) |
| `simbad-resolver-sesame` | SIMBAD Sesame client (name resolve) |
| `simbad-resolver-cache` | the `Cache` trait (pluggable interface) |
| `simbad-resolver-cache-memory` | in-memory `Cache` impl (dashmap) |
| `simbad-resolver-cache-sqlite` | SQLite `Cache` impl (sqlx), owns migrations |
| `simbad-resolver-caldwell` | Caldwell C1–C109 → NGC/IC map |
| `simbad-resolver` | facade + orchestration (cache-first, sticky override, async batch) |

Rationale: keep the pure/sync core free of network/DB weight; let TAP and Sesame
be co-equal, independently selectable resolvers; isolate the heavy `sqlx`
surface behind an opt-in cache impl so a lean install pulls only what it needs.

### Cache: pluggable trait, not a bundled store

The `Cache` is a **domain dedup/typeahead store** (persist `ResolvedIdentity`
by SIMBAD oid, normalized-alias index, source precedence, ranked search), not a
generic KV/TTL cache. Off-the-shelf eviction caches (`moka`, `lru`,
`quick_cache`) are the wrong abstraction. We consume storage **primitives**:
`dashmap` for the in-memory impl, `sqlx` (async, migrations, rustls) for SQLite
(`rusqlite` is a lighter sync alternative considered but not chosen — it needs
`spawn_blocking` in our async context).

### HTTP

`reqwest` with `default-features = false` + `rustls` (no OpenSSL). Consequence:
no high-level query builder, so ADQL is percent-encoded into the URL by hand
(carried over from the origin implementation).

### Seed

**No bundled seed** in the core install. This project is purely SIMBAD
resolution; curated seed catalogs (Messier/Caldwell/NGC/IC/…) ship as **separate
packages** if/when needed.

### Configurable, not hardcoded

- The stable-id UUIDv5 **namespace** is caller-supplied (the origin app hardcoded
  `"astro-plan.targets"`; that value is load-bearing for its existing ids, so it
  passes it explicitly).
- The **User-Agent** is caller-supplied with a neutral default.
- The SIMBAD endpoint is a single configurable default.

## Ecosystem

`simbad-resolver` is **upstream** (name → identity). It composes with:

- **`astro-angle`** — shared coordinate/angle primitives (`Equatorial`, angles,
  sexagesimal). Adopted as the coordinate type once available; decimal-degree
  seam until then.
- **`target-match`** (formerly `astro-target-id`) — **downstream**: pointing +
  FOV → ranked-by-separation candidates. It consumes our output; the optics/FOV
  geometry lives there. We provide **cone-search** (a SIMBAD-side position
  query), which is complementary to `target-match`'s local ranking, not a
  duplicate of it.

## Consequences

- More crates to version/publish, mitigated by a shared workspace and
  `release-please`.
- Consumers wanting the batteries-included experience add `simbad-resolver` with
  the impl features they need; type-only consumers depend on
  `simbad-resolver-core`.
