//! Pure, sync foundation crate for `simbad-resolver`.
//!
//! Holds the resolved-identity types ([`ObjectType`], [`TargetSource`],
//! [`AliasKind`], [`ResolvedAlias`], [`ResolvedIdentity`], [`PositionMatch`]),
//! the typed [`ResolveError`], the [`normalize`]/[`identity`] pipelines, the
//! [`wire`] TSV-parsing helpers, and the [`Resolver`]/[`PositionResolver`]
//! seam (plus [`OfflineResolver`] and, behind `feature = "test-fixture"`,
//! [`FakeResolver`]).
//!
//! This crate MUST NOT depend on an async runtime or an HTTP/SQL client
//! (no `tokio`, `reqwest`, or `sqlx`): every network-/DB-backed resolver
//! implementation lives in a downstream crate (`simbad-resolver-tap`,
//! `-sesame`, `-cache-sqlite`) that depends on this one, never the reverse.
#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod identity;
pub mod normalize;
pub mod resolver;
pub mod types;
pub mod wire;

pub use config::SimbadConfig;
pub use error::ResolveError;
#[cfg(any(test, feature = "test-fixture"))]
pub use resolver::FakeResolver;
pub use resolver::{OfflineResolver, PositionResolver, Resolver};
pub use types::{
    map_otype, AliasKind, ObjectType, PositionMatch, ResolvedAlias, ResolvedIdentity, TargetSource,
};
