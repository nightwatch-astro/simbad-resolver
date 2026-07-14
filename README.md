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
storage), see the
**[guide](https://docs.rs/simbad-resolver/latest/simbad_resolver/guide/index.html)**
(also in this repo at `docs/guide.md`).

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

Caldwell designations (`C 14`) are not SIMBAD identifiers; the facade
translates them to the underlying catalog designation via
[`caldwell::caldwell_to_designation`](https://docs.rs/simbad-resolver/latest/simbad_resolver/caldwell/fn.caldwell_to_designation.html)
before resolving, then binds the original `C n` as an alias.

## Positions

Resolving by sky position (cone search) is the
[`PositionResolver`](https://docs.rs/simbad-resolver/latest/simbad_resolver/trait.PositionResolver.html)
capability, implemented by `TapResolver`. Its
[`resolve_position`](https://docs.rs/simbad-resolver/latest/simbad_resolver/trait.PositionResolver.html#tymethod.resolve_position)
returns the objects within `radius_deg` of an ICRS position, nearest first, as
[`PositionMatch`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.PositionMatch.html)
values. This hits the live endpoint, so it needs network access:

```rust,no_run
use simbad_resolver::{PositionResolver, TapResolver};

# async fn run() -> Result<(), simbad_resolver::ResolveError> {
let resolver = TapResolver::with_defaults()?;
// Objects within 0.05° of M 31's ICRS position, nearest first (max 5).
let matches = resolver.resolve_position(10.684_708, 41.268_75, 0.05, 5).await?;
for m in &matches {
    println!("{} at {:.4}°", m.identity.primary_designation, m.separation_deg);
}
# Ok(())
# }
```

## Search

[`SimbadResolver::search`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.SimbadResolver.html#method.search)
is a local, network-free typeahead over cached aliases, ranked exact > prefix >
substring. Enabling fuzzy matching with
[`ResolverConfig::with_fuzzy`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.ResolverConfig.html#method.with_fuzzy)
fills remaining result slots with token-set similarity hits in the
[`RANK_FUZZY`](https://docs.rs/simbad-resolver/latest/simbad_resolver/constant.RANK_FUZZY.html)
tier. This does not change `resolve`, which stays exact-normalized.

## Overrides

[`SimbadResolver::apply_override`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.SimbadResolver.html#method.apply_override)
binds a chosen canonical target as authoritative: it adds the supplied alias
and marks the row sticky (`source = user-override`), so a later re-resolve does
not overwrite it. See the
[guide](https://docs.rs/simbad-resolver/latest/simbad_resolver/guide/index.html)
for an example.

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
each exposing `.cache()` and `.queue()` over one database). Open the `Store`
yourself to share one database between a `SimbadResolver` and a `BatchResolver`:

```rust,no_run
use simbad_resolver::{
    BatchResolver, CacheBackend, ResolverConfig, SimbadResolver, Store, TapResolver,
};

# fn run() -> Result<(), Box<dyn std::error::Error>> {
let store = Store::open("targets.redb")?;
let config = ResolverConfig::new("your.namespace");

// Both operate over the same rows: the facade caches resolved targets, the
// batch resolver drains its queue into that same cache.
let facade = SimbadResolver::new(
    TapResolver::with_defaults()?,
    CacheBackend::custom(store.cache()),
    config.clone(),
)?;
let batch =
    BatchResolver::new(TapResolver::with_defaults()?, store.cache(), store.queue(), config);
# let _ = (facade, batch);
# Ok(())
# }
```

## Coordinates

Resolved-object types
([`ResolvedIdentity`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.ResolvedIdentity.html),
[`CachedTarget`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.CachedTarget.html),
and the cone-search
[`PositionMatch`](https://docs.rs/simbad-resolver/latest/simbad_resolver/struct.PositionMatch.html))
keep their canonical `ra_deg`/`dec_deg` (`f64`, ICRS J2000 degrees) and also
expose a typed accessor:

```rust
# use simbad_resolver::ResolvedIdentity;
# fn coordinates(target: &ResolvedIdentity) -> Result<(), Box<dyn std::error::Error>> {
let eq = target.position()?; // skymath::Equatorial: validated RA/Dec carrying its epoch
# let _ = eq;
# Ok(())
# }
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

When a query does not resolve, the
[`Resolution::Unresolved`](https://docs.rs/simbad-resolver/latest/simbad_resolver/enum.Resolution.html)
`reason` is a
[`UnresolvedReason`](https://docs.rs/simbad-resolver/latest/simbad_resolver/enum.UnresolvedReason.html):
`Offline` (backend unreachable/timed out/disabled — retry later; cached objects
still resolve), `Unknown` (no such object — give up), or `Ambiguous` (several
distinct objects — disambiguate the query).

See the [guide](https://docs.rs/simbad-resolver/latest/simbad_resolver/guide/index.html)
for a walkthrough that also covers testing without the network
(`OfflineResolver`/`FakeResolver`) and batch resolution.

```toml
[dependencies]
simbad-resolver = "0.2"
```

## Attribution

This library queries SIMBAD, operated at CDS, Strasbourg, France. Applications
that display resolved data should credit CDS and send an identifying
`User-Agent` header (configurable via `SimbadConfig`).

## License

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

This project is licensed under the Mozilla Public License 2.0 — see [LICENSE](LICENSE) for details.

You can use this library in closed-source projects. If you modify any of the source files in this library, the modified files must be made available under the MPL-2.0 when distributed.
