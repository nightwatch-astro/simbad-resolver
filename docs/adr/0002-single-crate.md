# ADR 0002 — Collapse to a single crate

- Status: accepted (supersedes the "Crate split (granular)" decision in ADR-0001)
- Date: 2026-07-11
- Deciders: Sjors Robroek

## Context

`0.1.0` was built as an 8-crate workspace (ADR-0001). In practice the crates
form one cohesive library: consumers depend only on the facade, and the split
imposed 8-way version/publish coordination (lockstep release-please, crates.io
new-crate rate limits) for little benefit at this size. The project is
greenfield and pre-1.0, so it can be restructured freely.

## Decision

Collapse the workspace into a single crate, `simbad-resolver`, with modules
where crates used to be.

### Dependencies — bake in the essentials, gate only SQLite

The library's purpose is resolving against SIMBAD over the network, so the HTTP
stack and both resolvers are not optional:

- **Always compiled:** the core types/normalization/identity, `reqwest`, the TAP
  and Sesame resolvers, the in-memory cache (`dashmap`), and Caldwell.
- **Optional (`sqlite` feature, off by default):** the SQLite backend (`sqlx`).
  `sqlx` is heavy (long compile, large transitive tree) and many consumers only
  need in-memory caching — this is the one place "pay for what you use" earns
  its keep.

The feature surface is exactly one flag: `sqlite`.

### Namespace — flat and ergonomic

Primary types are re-exported at the crate root so consumers write
`use simbad_resolver::{SimbadResolver, TapResolver, MemoryCache, ObjectType, ResolveError};`.
Drop the redundant `Simbad` prefix on the network resolvers
(`SimbadTapResolver` → `TapResolver`, `SimbadSesameResolver` → `SesameResolver`).
`caldwell` stays a small public module.

### Release

Single-package release-please (no `cargo-workspace` / `linked-versions`); publish
is a plain `cargo publish` via crates.io Trusted Publishing (OIDC).

## Consequences

- One version, one changelog, one crates.io entry, one publish — no lockstep, no
  new-crate rate-limit chains.
- The seven `simbad-resolver-*` sub-crates published at `0.1.0` are **yanked**
  and superseded by the single `simbad-resolver` crate, first published at
  `0.1.0` under the (previously unpublished) facade name.
- No sub-crate remains separately consumable; acceptable, as none had
  independent consumers.
- Supersedes only the "Crate split (granular)" section of ADR-0001. The rest of
  0001 still holds: the cache is a domain dedup/typeahead store (not a generic
  KV/TTL cache), `reqwest` + rustls, no bundled seed, and a caller-supplied id
  namespace + User-Agent.
