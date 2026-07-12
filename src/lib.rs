//! # simbad-resolver
//!
//! A **SIMBAD astronomical target resolver** for Rust.
//!
//! Resolves astronomical names, designations, and positions to canonical
//! identities via SIMBAD, and provides the higher-level orchestration:
//! cache-first [`SimbadResolver::resolve`], sticky
//! [`SimbadResolver::apply_override`], local [`SimbadResolver::search`], and the
//! async [`BatchResolver`]. Persistence is a single redb-backed [`Store`] that
//! serves both durable (file) and ephemeral (in-memory) modes.
//!
//! Resolved-object types ([`ResolvedIdentity`], [`CachedTarget`], and cone-search
//! [`PositionMatch`]) also expose a typed `skymath::Equatorial` position via
//! `position()` alongside their raw `ra_deg`/`dec_deg` — SIMBAD's ICRS output is
//! treated as J2000 at planning grade (≤ ~1 arcminute).
//!
//! ```no_run
//! use simbad_resolver::{CacheBackend, Resolution, ResolverConfig, SimbadResolver, TapResolver};
//! # async fn demo() -> Result<(), simbad_resolver::Error> {
//! let resolver = TapResolver::with_defaults().expect("client");
//! // Zero-config ephemeral cache; use `CacheBackend::file("targets.redb")` to persist,
//! // or `CacheBackend::custom(my_cache)` to bring your own backend.
//! let facade =
//!     SimbadResolver::new(resolver, CacheBackend::InMemory, ResolverConfig::new("my-app.targets"))?;
//! if let Resolution::Resolved(t) = facade.resolve("M 31").await? {
//!     println!("{} @ ({}, {})", t.primary_designation, t.ra_deg, t.dec_deg);
//! }
//! # Ok(()) }
//! ```
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
    UpsertOutcome, RANK_EXACT, RANK_FUZZY, RANK_PREFIX, RANK_SUBSTRING,
};
pub use crate::config::{ResolverConfig, SimbadConfig};
pub use crate::error::{Error, ResolveError};
pub use crate::orchestrate::{
    CacheBackend, FileCache, Resolution, SimbadResolver, UnresolvedReason,
};
pub use crate::resolver::{OfflineResolver, PositionResolver, Resolver};
pub use crate::sesame::SesameResolver;
pub use crate::tap::TapResolver;
pub use crate::types::{
    map_otype, AliasKind, ObjectType, PositionMatch, ResolvedAlias, ResolvedIdentity, TargetSource,
};

#[cfg(any(test, feature = "test-util"))]
pub use crate::resolver::FakeResolver;
