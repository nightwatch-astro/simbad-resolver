# Handover — simbad-resolver

**Date**: 2026-07-11 · **Repo**: https://github.com/srobroek/simbad-resolver (public)
**Branches**: `main` (integrated) and `001-simbad-target-resolution` (feature) — both pushed.

## What this is

`simbad-resolver` — a generic, embeddable Rust library that resolves an
astronomical name/designation (`M31`, `NGC 224`, `Andromeda Galaxy`, `C 14`) or a
sky position into a canonical target identity (ICRS J2000 coords, object type,
alias set, stable id) against SIMBAD, with a pluggable cache. Extracted and
generalized from `astro-plan`'s `targeting`/`targeting_resolver`.

## Status: COMPLETE and verified

- **8 crates** build; **149 tests pass**; `clippy --all-targets --all-features -D warnings` clean; `cargo fmt --check` clean; `cargo doc` clean. CI (`.github/workflows/ci.yml`) enforces all of these.
- Live SIMBAD tests exist but are `#[ignore]`-gated (network): `just test-live`.

| Crate | Role | Tests |
|---|---|---|
| `simbad-resolver-core` | pure types, normalize, identity (configurable ns), `Resolver`/`PositionResolver` traits, `Offline`/`FakeResolver`, wire helpers | 53 |
| `simbad-resolver-caldwell` | C1–C109 map + `parse_caldwell_number` | 7 |
| `simbad-resolver-tap` | `SimbadTapResolver`: name resolve (2 round-trips) + cone search | 11 (+3 live) |
| `simbad-resolver-sesame` | `SimbadSesameResolver`: Sesame name resolve + optional enrichment | 14 (+1 live) |
| `simbad-resolver-cache` | `Cache` + `Queue` async traits | — |
| `simbad-resolver-cache-memory` | dashmap `Cache`+`Queue` | 26 |
| `simbad-resolver-cache-sqlite` | sqlx `Cache`+`Queue` + migrations | 28 |
| `simbad-resolver` | facade: cache-first resolve, sticky override, async batch; feature-gated re-exports | 9 (+doctest) |

## How to work with it

```bash
just build      # cargo build --workspace --all-features
just test       # offline suite (149 tests)
just lint       # fmt --check + clippy -D warnings
just test-live  # ignored live SIMBAD tests (needs network)
```

Usage: `SimbadResolver::new(resolver, cache, ResolverConfig::new("your.ns"))` then
`.resolve(query)` → `Resolution::{Resolved(CachedTarget), Unresolved{reason}}`. See
`crates/simbad-resolver/src/lib.rs` doctest and `tests/integration.rs`.

## Spec / process artifacts

Full SpecKit spec under `specs/001-simbad-target-resolution/`: `spec.md`
(FR-001…FR-020, SC-001…SC-007), `plan.md`, `research.md`, `data-model.md`,
`contracts/`, `quickstart.md`, `tasks.md`. Architecture rationale in
`docs/adr/0001-stack-and-architecture.md`. Autonomous decisions + ambiguities in
`DECISIONS.md`.

## Open items / inputs needed from you (see DECISIONS.md)

1. **Sesame XML — verify live (highest priority).** The Sesame parser uses a
   *hand-built* fixture, not a live capture. Run
   `cargo test -p simbad-resolver-sesame -- --ignored` (and the TAP live tests)
   on a networked host and refresh `crates/simbad-resolver-sesame/src/parse.rs`
   if the real `-oxp` schema differs. Parser is tolerant but this is unverified.
2. **crates.io names** not reserved — confirm before first publish. `release-please`
   is installed; crate versions are `0.0.0` (bump on first release).
3. **Dual-license**: currently Apache-2.0 only; Rust convention is dual
   `MIT OR Apache-2.0` — say the word to add MIT.
4. **`astro-angle`** adoption for the coordinate type (currently `f64` ra/dec seam)
   once that crate exists; and **`simbad-seed` / `simbad-seed-builder`** as separate
   packages (deliberately out of this core).

## Ecosystem (fixed direction of flow)

`simbad-resolver` is **upstream** (name → identity). `target-match` (formerly
`astro-target-id`) is **downstream** (pointing + FOV → ranked candidates) and
consumes this crate's output. Both will speak `astro-angle` coordinate types.
Our cone search is a complementary SIMBAD-side query, not a duplicate of
`target-match`'s local ranking. Do not add local FOV geometry here.

## Notable design choices (all in DECISIONS.md)

Granular 8-crate split; async `Resolver`/`Cache`/`Queue` traits; caller-owned
config (no persisted settings table); configurable id namespace; Sesame standalone
+ optional enrichment; Caldwell translation in the facade; batch drain is
sequential/polite (parallelism is future work); dependencies unconstrained (dropped
astro-plan's hand-rolled percent-encoding / https-check / `domain_core` / `strsim`).
