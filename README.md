# simbad-resolver

[![CI](https://github.com/nightwatch-astro/simbad-resolver/actions/workflows/ci.yml/badge.svg)](https://github.com/nightwatch-astro/simbad-resolver/actions/workflows/ci.yml)
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

It is a single crate with no required feature flags: the network resolvers and a
redb-backed store (durable or in-memory) are always available.

## Documentation

Full API documentation is generated from the source and published on docs.rs:
**[docs.rs/simbad-resolver](https://docs.rs/simbad-resolver)**.

```bash
cargo doc --open
```

## Resolving

Two SIMBAD backends are available, both built in:

- **`TapResolver`** queries the SIMBAD TAP service with ADQL. It returns SIMBAD
  object ids, object types, the full alias set, and supports cone search by
  position.
- **`SesameResolver`** queries the CDS Sesame name resolver, which aggregates
  SIMBAD, NED, and VizieR. It resolves a broader range of names but returns
  coordinates and a primary designation only; object type and aliases can be
  filled in by supplying a `TapResolver` as an enricher.

## Storage

The `Cache` and `Queue` traits are the persistence abstraction; bring your own
implementation, or use the built-in `Store`, backed by
[redb](https://crates.io/crates/redb) (a pure-Rust embedded ACID store):

- `Store::open(path)` — durable, file-backed; survives restarts.
- `Store::in_memory()` — ephemeral, nothing written to disk.

Both expose `.cache()` and `.queue()` handles over the same database.

## Coordinates

Resolved-object types (`ResolvedIdentity`, `CachedTarget`, and the cone-search
`PositionMatch`) keep their canonical `ra_deg`/`dec_deg` (`f64`, ICRS J2000
degrees) and also expose a typed accessor:

```rust,ignore
let eq = target.position()?; // skymath::Equatorial: validated RA/Dec carrying its epoch
```

`position()` returns a [skymath](https://crates.io/crates/skymath) `Equatorial`
— the shared, domain-validated coordinate type — so downstream consumers
(ranking, formatting) share one representation with no conversion. SIMBAD's ICRS
output is treated as J2000 at planning grade (≤ ~1 arcminute); the raw `f64`
fields remain the source of truth.

## Usage

```rust
use simbad_resolver::{Resolution, ResolverConfig, SimbadResolver, Store, TapResolver};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let resolver = TapResolver::with_defaults()?;
let store = Store::in_memory()?; // or Store::open("targets.redb")?
let facade = SimbadResolver::new(resolver, store.cache(), ResolverConfig::new("your.namespace"));

match facade.resolve("M31").await? {
    Resolution::Resolved(target) => {
        println!("{} at ({}, {})", target.primary_designation, target.ra_deg, target.dec_deg);
    }
    Resolution::Unresolved { reason, .. } => println!("unresolved: {reason:?}"),
}
# Ok(())
# }
```

```toml
[dependencies]
simbad-resolver = "0.1"
```

## Attribution

This library queries SIMBAD, operated at CDS, Strasbourg, France. Applications
that display resolved data should credit CDS and send an identifying
`User-Agent` header (configurable via `SimbadConfig`).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
