# simbad-resolver

Generic, embeddable **SIMBAD astronomical target resolver** for Rust.

`simbad-resolver` turns an astronomical **name or catalog designation** (`M31`,
`NGC 224`, `Andromeda Galaxy`, `C 14`) — or a **sky position** — into a single
canonical target **identity**: ICRS J2000 coordinates, a closed object-type
classification, the full alias/designation set, and a stable id. It resolves
on-demand against SIMBAD and never fabricates coordinates.

> Status: **early scaffold.** The architecture and requirements are captured in
> [`specs/`](specs/); crates are implemented incrementally against that spec.

## Why

Extracted and generalized from the target-resolution subsystem of an astronomy
imaging app, decoupled from that app's database schema, identity namespace, and
branding so it can be embedded in any Rust project that needs to answer *"what
astronomical object is this?"*.

## Design at a glance

- **Two first-class resolvers.** `simbad-resolver-tap` (SIMBAD TAP/ADQL —
  precise, structured, cone-search) and `simbad-resolver-sesame` (SIMBAD Sesame
  — broader name coverage aggregating SIMBAD/NED/VizieR). Pick per use case.
- **Pluggable cache.** A `Cache` trait with `-cache-memory` (dashmap) and
  `-cache-sqlite` (sqlx) implementations; bring your own backend.
- **Never fabricate.** Unknown/ambiguous/offline → an explicit unresolved
  outcome, never a guessed coordinate.
- **Configurable identity.** The stable-id UUID namespace and the HTTP
  User-Agent are caller-supplied, not baked in.
- **Async, polite.** Cache-first single resolve plus an async batch resolver
  with transient-vs-miss retry semantics for resolving many names without
  hammering CDS.

See [`docs/adr/0001-stack-and-architecture.md`](docs/adr/0001-stack-and-architecture.md)
for the crate split and rationale.

## Workspace crates

| Crate | Role |
|---|---|
| `simbad-resolver` | **Main facade** — orchestration (cache-first, sticky override, async batch) + re-exports |
| `simbad-resolver-core` | Pure types (`ObjectType`, `TargetSource`, `ResolvedIdentity`), normalization, identity, the `Resolver` trait |
| `simbad-resolver-tap` | SIMBAD TAP client — resolve-by-name + cone-search |
| `simbad-resolver-sesame` | SIMBAD Sesame client — resolve-by-name |
| `simbad-resolver-cache` | The `Cache` trait (pluggable interface) |
| `simbad-resolver-cache-memory` | In-memory `Cache` impl (dashmap) |
| `simbad-resolver-cache-sqlite` | SQLite `Cache` impl (sqlx), owns its migrations |
| `simbad-resolver-caldwell` | Caldwell C1–C109 → NGC/IC designation map |

## Ecosystem

`simbad-resolver` is one crate in a small, composable ecosystem. It is
**upstream** — it produces objects; it never consumes them.

```
   name / browse catalog                    frame pointing + FOV
           │                                        │
           ▼                                        ▼
  ┌────────────────────┐   {id, ra, dec}   ┌────────────────────┐
  │   simbad-resolver   │ ────────────────▶ │    target-match     │
  │ name → identity,    │   candidates      │ rank by angular sep,│
  │ SIMBAD TAP+Sesame,  │                   │ FOV/radius geometry │
  │ pluggable cache     │                   │ → nearest in-frame  │
  └────────────────────┘                   └────────────────────┘
           └──────────── both speak astro-angle ──────────────┘
```

- **[`astro-angle`](https://github.com/srobroek/astro-angle)** — shared
  coordinate/angle primitives (`Equatorial`, angles, sexagesimal parse/format).
  `simbad-resolver` adopts these as its coordinate type; until `astro-angle`
  lands, coordinates are plain decimal degrees behind a conversion seam.
- **[`target-match`](https://github.com/srobroek/target-match)** (formerly
  `astro-target-id`) — the **downstream** consumer. Given a frame pointing, a
  field-of-view/radius, and a list of candidate positions, it ranks them by
  angular separation and returns those that fall on the frame. It takes a
  radius in degrees; the optics→FOV→radius geometry lives there, not here. Our
  cone-search and `list`/search output feed straight into it.

The direction of flow is fixed: `simbad-resolver` answers *"what is this
name?"* and owns catalog identity; `target-match` answers *"which of these did
this frame capture?"*. Objects are never passed **into** the resolver.

## Attribution

This library queries **SIMBAD** operated at CDS, Strasbourg, France. Consumers
displaying resolved data should credit CDS per its usage norms and supply an
identifying `User-Agent`.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
