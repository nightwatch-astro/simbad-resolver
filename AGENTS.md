# AGENTS.md

Guidance for agentic tools working in this repository.

## Project

**simbad-resolver** — a generic, embeddable SIMBAD astronomical target resolver
for Rust. It resolves an astronomical name/designation (or a sky position) to a
canonical identity (ICRS J2000 coordinates, object type, alias set, stable id)
against SIMBAD, with a pluggable cache. Extracted and generalized from an
astronomy imaging app's target-resolution subsystem.

## Architecture

A granular Cargo workspace; the main installable crate is `simbad-resolver`.
See `docs/adr/0001-stack-and-architecture.md` and `specs/` for the crate split,
requirements, and rationale. Core principles:

- **Never fabricate** coordinates/identity — unknown/ambiguous/offline is an
  explicit unresolved outcome.
- **Two first-class resolvers**: TAP (structured, cone-search) and Sesame
  (broad name coverage). The `Resolver` trait is the seam.
- **Pluggable `Cache`** trait with memory + SQLite impls.
- **Configurable, not hardcoded**: UUID id-namespace and User-Agent are
  caller-supplied.

## Ecosystem boundary (do not blur it)

- `simbad-resolver` is **upstream** (name → identity; produces objects).
- `target-match` (formerly `astro-target-id`) is **downstream** (pointing + FOV
  → ranked candidates; consumes objects). Do **not** add local nearest-neighbour
  / FOV geometry here — that belongs to `target-match`.
- `astro-angle` provides shared coordinate/angle primitives both crates speak.
  Adopt `astro_angle::Equatorial` for coordinates once it exists; use a decimal-
  degree seam until then.

## Working style

- Spec-driven (SpecKit). Substantial work starts from a spec in `specs/`.
- Rust 2021, `cargo fmt` + `cargo clippy` clean, tests for every behavior.
- SIMBAD is an external network service: keep the network path behind the
  `Resolver` trait so unit tests run offline with a fake.
