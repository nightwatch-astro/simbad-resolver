# simbad-resolver

[![CI](https://github.com/nightwatch-astro/simbad-resolver/actions/workflows/ci.yml/badge.svg)](https://github.com/nightwatch-astro/simbad-resolver/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/simbad-resolver.svg)](https://crates.io/crates/simbad-resolver)
[![docs.rs](https://img.shields.io/docsrs/simbad-resolver)](https://docs.rs/simbad-resolver)

A Rust library for resolving astronomical object names, catalog designations,
and sky positions to canonical target identities using SIMBAD.

Given an input such as `M31`, `NGC 224`, `Andromeda Galaxy`, `C 14`, or a sky
position, `simbad-resolver` queries SIMBAD and returns a single canonical
identity: a stable id, ICRS J2000 coordinates, an object-type classification,
a Johnson V magnitude when SIMBAD has one (`v_mag`, `None` otherwise), the
object's alias set, and the provenance of the record.

Resolved identities are stored in a pluggable cache, so repeated lookups are
served locally instead of re-querying SIMBAD. An async batch resolver
processes many names against a durable queue.

It is a single crate with no required feature flags: the network resolvers and a
redb-backed store (durable or in-memory) ship in the default build.

## Documentation

Full API documentation is generated from the source and published on docs.rs:
**[docs.rs/simbad-resolver](https://docs.rs/simbad-resolver)**. For a
task-oriented walkthrough (resolving a name, testing offline, batching,
storage), see **[docs/guide.md](docs/guide.md)**.

```bash
cargo doc --open
```

## Resolving

Two SIMBAD backends are available, both built in:

- **[`TapResolver`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.TapResolver.html)**
  queries the SIMBAD TAP service with ADQL. It returns SIMBAD object ids,
  object types, the full alias set, a Johnson V magnitude (via a
  `LEFT OUTER JOIN` on `allfluxes`, `None` when the object has no V photometry),
  and supports cone search by position.
- **[`SesameResolver`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.SesameResolver.html)**
  queries the CDS Sesame name resolver, which aggregates SIMBAD, NED, and
  VizieR. It resolves a broader range of names but returns coordinates and a
  primary designation only; object type and aliases can be filled in by
  supplying a `TapResolver` as an
  [enricher](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.SesameResolver.html#method.with_enricher).

Both implement the
[`Resolver`](https://docs.rs/simbad-resolver/latest/simbad_resolver/trait.Resolver.html)
trait, so either can back a
[`SimbadResolver`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.SimbadResolver.html)
facade or a
[`BatchResolver`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.BatchResolver.html).

## Storage

The facade selects its cache with a
[`CacheBackend`](https://docs.rs/simbad-resolver/latest/simbad_resolver/enum.CacheBackend.html),
chosen at construction — there is no separate "enable caching" step, because
the cache is load-bearing (resolve returns cached rows). The built-in backend
is [redb](https://crates.io/crates/redb) (a pure-Rust embedded ACID store):

- `CacheBackend::InMemory` — ephemeral, nothing written to disk (also `Default`).
- [`CacheBackend::file("targets.redb")`](https://docs.rs/simbad-resolver/latest/simbad_resolver/enum.CacheBackend.html#method.file)
  — durable, file-backed; survives restarts.
- [`CacheBackend::custom(my_cache)`](https://docs.rs/simbad-resolver/latest/simbad_resolver/enum.CacheBackend.html#method.custom)
  — any
  [`Cache`](https://docs.rs/simbad-resolver/latest/simbad_resolver/trait.Cache.html)
  implementation you supply.

For an uncached one-shot lookup, call a bare `Resolver` (e.g. `TapResolver`)
directly instead of the facade.

Under the hood the built-in variants open a
[`Store`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.Store.html)
(still public:
[`Store::open`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.Store.html#method.open)
/
[`Store::in_memory`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.Store.html#method.in_memory),
each exposing `.cache()` and `.queue()` over one database) — use it directly to
share a single database between a `SimbadResolver` and a `BatchResolver`, or to
tune the backend.

## Coordinates

Resolved-object types
([`ResolvedIdentity`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.ResolvedIdentity.html),
[`CachedTarget`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.CachedTarget.html),
and the cone-search
[`PositionMatch`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.PositionMatch.html))
keep their canonical `ra_deg`/`dec_deg` (`f64`, ICRS J2000 degrees) and also
expose a typed accessor:

```rust,ignore
let eq = target.position()?; // skymath::Equatorial: validated RA/Dec carrying its epoch
```

`position()` returns a [`skymath::Equatorial`](https://docs.rs/skymath/latest/skymath/struct.Equatorial.html)
— this crate depends on [skymath](https://crates.io/crates/skymath) for that
shared, domain-validated coordinate type, so downstream consumers (ranking,
formatting) share one representation with no conversion. SIMBAD's ICRS output
is treated as J2000 at planning grade (≤ ~1 arcminute); the raw `f64` fields
remain the source of truth.

## Usage

This calls the live SIMBAD TAP endpoint, so it needs network access to run.

```rust,no_run
use simbad_resolver::{CacheBackend, Resolution, ResolverConfig, SimbadResolver, TapResolver};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let resolver = TapResolver::with_defaults()?;
// Ephemeral cache; `CacheBackend::file("targets.redb")` persists across restarts.
let facade =
    SimbadResolver::new(resolver, CacheBackend::InMemory, ResolverConfig::new("your.namespace"))?;

match facade.resolve("M31").await? {
    Resolution::Resolved(target) => {
        println!("{} at ({}, {})", target.primary_designation, target.ra_deg, target.dec_deg);
    }
    Resolution::Unresolved { reason, .. } => println!("unresolved: {reason:?}"),
}
# Ok(())
# }
```

See [docs/guide.md](docs/guide.md) for a walkthrough that also covers testing
without the network (`OfflineResolver`/`FakeResolver`) and batch resolution.

```toml
[dependencies]
simbad-resolver = "0.2"
```

## Attribution

This library queries SIMBAD, operated at CDS, Strasbourg, France. Applications
that display resolved data should credit CDS and send an identifying
`User-Agent` header (configurable via `SimbadConfig`).

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
