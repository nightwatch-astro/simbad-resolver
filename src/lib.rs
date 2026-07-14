// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The README is the docs.rs crate-root landing page; its own `rust` fences are
// this crate's doctests (network ones `no_run`, offline ones runnable — see
// README.md's per-section notes). Keep prose in the README, not duplicated here.
#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]

// `docs/guide.md`, rendered as its own docs.rs page (this module is otherwise
// empty — it exists only to give the included guide somewhere to render).
#[doc = include_str!("../docs/guide.md")]
pub mod guide {}

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
pub use crate::cache::redb::{BatchDurability, RedbCache, RedbQueue, Store};
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
