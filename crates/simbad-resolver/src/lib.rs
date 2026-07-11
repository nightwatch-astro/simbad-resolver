//! # simbad-resolver
//!
//! Generic, embeddable **SIMBAD astronomical target resolver** for Rust.
//!
//! The main installable facade of the `simbad-resolver-*` workspace. It
//! re-exports the resolver ecosystem and provides the higher-level
//! orchestration: cache-first [`SimbadResolver::resolve`], sticky
//! [`SimbadResolver::apply_override`], local [`SimbadResolver::search`], and the
//! async [`BatchResolver`].
//!
//! ```no_run
//! use simbad_resolver::{ResolverConfig, SimbadResolver, Resolution};
//! # async fn demo() -> Result<(), simbad_resolver::Error> {
//! # #[cfg(all(feature = "tap", feature = "memory"))] {
//! let resolver = simbad_resolver::tap::SimbadTapResolver::with_defaults()
//!     .expect("client");
//! let cache = simbad_resolver::memory::MemoryCache::new();
//! let facade = SimbadResolver::new(resolver, cache, ResolverConfig::new("my-app.targets"));
//! if let Resolution::Resolved(t) = facade.resolve("M 31").await? {
//!     println!("{} @ ({}, {})", t.primary_designation, t.ra_deg, t.dec_deg);
//! }
//! # }
//! # Ok(()) }
//! ```
//!
//! ## Ecosystem
//!
//! `simbad-resolver` is **upstream** (name → identity; produces objects). It
//! composes with two sibling crates:
//!
//! - [`astro-angle`] — shared coordinate/angle primitives (`Equatorial`,
//!   sexagesimal). Adopted as the coordinate type once it exists; decimal-degree
//!   values behind a conversion seam until then.
//! - [`target-match`] (formerly `astro-target-id`) — **downstream**: pointing +
//!   FOV → ranked candidates. It *consumes* this crate's output; nothing flows
//!   back into the resolver. Our cone search ([`PositionResolver`]) is a
//!   complementary upstream-service query, not a duplicate of `target-match`'s
//!   local ranking.
//!
//! ```text
//!    name / browse catalog                    frame pointing + FOV
//!            │                                        │
//!            ▼                                        ▼
//!   ┌────────────────────┐   {id, ra, dec}   ┌────────────────────┐
//!   │   simbad-resolver   │ ────────────────▶ │    target-match     │
//!   └────────────────────┘   candidates      └────────────────────┘
//!            └──────────── both speak astro-angle ──────────────┘
//! ```
//!
//! [`astro-angle`]: https://github.com/srobroek/astro-angle
//! [`target-match`]: https://github.com/srobroek/target-match
#![forbid(unsafe_code)]

mod batch;
mod config;
mod error;
mod orchestrate;

pub use batch::{BatchResolver, DrainSummary};
pub use config::ResolverConfig;
pub use error::Error;
pub use orchestrate::{Resolution, SimbadResolver, UnresolvedReason};

// ── Re-exports from the foundational crates ──────────────────────────────────

pub use simbad_resolver_cache::{
    Cache, CacheError, CachedTarget, PendingItem, PendingState, Queue, QueueError, SearchHit,
    UpsertOutcome,
};
pub use simbad_resolver_caldwell as caldwell;
pub use simbad_resolver_core::{
    map_otype, AliasKind, ObjectType, OfflineResolver, PositionMatch, PositionResolver,
    ResolveError, ResolvedAlias, ResolvedIdentity, Resolver, SimbadConfig, TargetSource,
};

// ── Feature-gated backend re-exports (batteries-included) ─────────────────────

/// In-memory cache/queue backend (feature `memory`).
#[cfg(feature = "memory")]
pub use simbad_resolver_cache_memory as memory;
/// Durable SQLite cache/queue backend (feature `sqlite`).
#[cfg(feature = "sqlite")]
pub use simbad_resolver_cache_sqlite as sqlite;
/// Broad-coverage SIMBAD Sesame resolver backend (feature `sesame`).
#[cfg(feature = "sesame")]
pub use simbad_resolver_sesame as sesame;
/// Precise SIMBAD TAP resolver backend (feature `tap`).
#[cfg(feature = "tap")]
pub use simbad_resolver_tap as tap;
