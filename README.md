# simbad-resolver

[![CI](https://github.com/srobroek/simbad-resolver/actions/workflows/ci.yml/badge.svg)](https://github.com/srobroek/simbad-resolver/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/simbad-resolver.svg)](https://crates.io/crates/simbad-resolver)
[![docs.rs](https://img.shields.io/docsrs/simbad-resolver)](https://docs.rs/simbad-resolver)

A Rust library for resolving astronomical object names, catalog designations,
and sky positions to canonical target identities using SIMBAD.

Given an input such as `M31`, `NGC 224`, `Andromeda Galaxy`, `C 14`, or a sky
position, `simbad-resolver` queries SIMBAD and returns a single canonical
identity: a stable id, ICRS J2000 coordinates, an object-type classification,
the object's alias set, and the provenance of the record. Resolved identities
are stored in a pluggable cache, so repeated lookups are served locally instead
of re-querying SIMBAD, and an async batch resolver processes many names against
a durable queue.

## Documentation

Full API documentation is generated from the source and published on docs.rs:

- **[docs.rs/simbad-resolver](https://docs.rs/simbad-resolver)** — the facade
  crate and its re-exports (start here).
- Each crate is documented individually; links to the sub-crates appear in the
  facade's documentation.

To build the documentation locally:

```bash
cargo doc --workspace --all-features --open
```

## Workspace crates

| Crate | Role |
|---|---|
| `simbad-resolver` | Facade: cache-first resolve, user-override precedence, async batch resolution, feature-gated re-exports of the crates below |
| `simbad-resolver-core` | Core types (`ObjectType`, `TargetSource`, `ResolvedIdentity`), query normalization, id derivation, and the `Resolver` trait |
| `simbad-resolver-tap` | SIMBAD TAP/ADQL client: resolve by name and cone search by position |
| `simbad-resolver-sesame` | CDS Sesame client: resolve by name (SIMBAD/NED/VizieR aggregation) |
| `simbad-resolver-cache` | The `Cache` and `Queue` traits |
| `simbad-resolver-cache-memory` | In-memory `Cache`/`Queue` implementation |
| `simbad-resolver-cache-sqlite` | SQLite `Cache`/`Queue` implementation |
| `simbad-resolver-caldwell` | Caldwell (C1–C109) to NGC/IC designation map |

## Resolver backends

- **TAP** (`simbad-resolver-tap`) queries the SIMBAD TAP service with ADQL. It
  returns SIMBAD object ids, object types, the full alias set, and supports cone
  search by position.
- **Sesame** (`simbad-resolver-sesame`) queries the CDS Sesame name resolver,
  which aggregates SIMBAD, NED, and VizieR. It resolves a broader range of names
  but returns coordinates and a primary designation only; object type and
  aliases can be filled in by supplying a TAP resolver as an enricher.

## Cache backends

The `Cache` trait is a durable store of resolved identities, keyed for
deduplication and typeahead search. Two implementations are provided —
in-memory (`simbad-resolver-cache-memory`) and SQLite
(`simbad-resolver-cache-sqlite`) — and callers can supply their own.

## Usage

```rust
use simbad_resolver::{Resolution, ResolverConfig, SimbadResolver};
use simbad_resolver::memory::MemoryCache;
use simbad_resolver_tap::SimbadTapResolver;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let resolver = SimbadResolver::new(
    SimbadTapResolver::with_defaults()?,
    MemoryCache::default(),
    ResolverConfig::new("your.namespace"),
);

match resolver.resolve("M31").await? {
    Resolution::Resolved(target) => {
        println!("{} at ({}, {})", target.primary_designation, target.ra_deg, target.dec_deg);
    }
    Resolution::Unresolved { reason, .. } => println!("unresolved: {reason:?}"),
}
# Ok(())
# }
```

Selecting backends via crate features:

```toml
[dependencies]
simbad-resolver = { version = "0.1", features = ["tap", "sqlite"] }
```

## Attribution

This library queries SIMBAD, operated at CDS, Strasbourg, France. Applications
that display resolved data should credit CDS and send an identifying
`User-Agent` header (configurable via `SimbadConfig`).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
