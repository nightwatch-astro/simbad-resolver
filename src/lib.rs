//! # simbad-resolver
//!
//! Generic, embeddable **SIMBAD astronomical target resolver** for Rust.
//!
//! Resolves astronomical names, designations, and positions to canonical
//! identities via SIMBAD, and provides the higher-level orchestration:
//! cache-first [`SimbadResolver::resolve`], sticky
//! [`SimbadResolver::apply_override`], local [`SimbadResolver::search`], and the
//! async [`BatchResolver`]. Persistence is a single redb-backed [`Store`] that
//! serves both durable (file) and ephemeral (in-memory) modes.
//!
//! ```no_run
//! use simbad_resolver::{Resolution, ResolverConfig, SimbadResolver, Store, TapResolver};
//! # async fn demo() -> Result<(), simbad_resolver::Error> {
//! let resolver = TapResolver::with_defaults().expect("client");
//! let store = Store::in_memory()?;
//! let facade = SimbadResolver::new(resolver, store.cache(), ResolverConfig::new("my-app.targets"));
//! if let Resolution::Resolved(t) = facade.resolve("M 31").await? {
//!     println!("{} @ ({}, {})", t.primary_designation, t.ra_deg, t.dec_deg);
//! }
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
//! [`astro-angle`]: https://github.com/srobroek/astro-angle
//! [`target-match`]: https://github.com/srobroek/target-match
#![forbid(unsafe_code)]

pub mod caldwell;
pub mod identity;
pub mod normalize;
pub mod wire;

mod batch;
mod cache;
mod config;
mod error;
mod orchestrate;
mod resolver;
mod sesame;
mod tap;
mod types;

pub use crate::batch::{BatchResolver, DrainSummary};
pub use crate::cache::redb::{RedbCache, RedbQueue, Store};
pub use crate::cache::{
    Cache, CacheError, CachedTarget, PendingItem, PendingState, Queue, QueueError, SearchHit,
    UpsertOutcome, RANK_EXACT, RANK_PREFIX, RANK_SUBSTRING,
};
pub use crate::config::{ResolverConfig, SimbadConfig};
pub use crate::error::{Error, ResolveError};
pub use crate::orchestrate::{Resolution, SimbadResolver, UnresolvedReason};
pub use crate::resolver::{OfflineResolver, PositionResolver, Resolver};
pub use crate::sesame::SesameResolver;
pub use crate::tap::TapResolver;
pub use crate::types::{
    map_otype, AliasKind, ObjectType, PositionMatch, ResolvedAlias, ResolvedIdentity, TargetSource,
};

#[cfg(any(test, feature = "test-util"))]
pub use crate::resolver::FakeResolver;
