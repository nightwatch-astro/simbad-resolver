//! Pluggable `Cache` and `Queue` traits for `simbad-resolver`.
//!
//! The [`Cache`] is a durable dedup/typeahead store of canonical target
//! identities (not a TTL/eviction cache). The [`Queue`] is the pending-work
//! store the async batch resolver drains. Both are `async` and object-safe;
//! backends live in sibling crates (`simbad-resolver-cache-memory`,
//! `simbad-resolver-cache-sqlite`) and callers may supply their own.
//!
//! Implemented per `specs/001-simbad-target-resolution/contracts/{cache,queue}.md`.
#![forbid(unsafe_code)]

use simbad_resolver_core::{ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};
use uuid::Uuid;

// ── Read models ──────────────────────────────────────────────────────────────

/// A cached canonical target plus its aliases, as read back from a [`Cache`].
///
/// Mirrors [`ResolvedIdentity`] but additionally carries the persisted
/// [`CachedTarget::id`] and [`CachedTarget::resolved_at`].
#[derive(Clone, Debug, PartialEq)]
pub struct CachedTarget {
    /// Persisted target id (UUIDv5 derived from the caller namespace).
    pub id: Uuid,
    /// SIMBAD physical-object id (dedup key) when resolved online.
    pub simbad_oid: Option<i64>,
    /// Canonical display designation.
    pub primary_designation: String,
    /// Curated common name when one exists.
    pub common_name: Option<String>,
    /// Closed object-type classification.
    pub object_type: ObjectType,
    /// Raw SIMBAD `otype` string (escape hatch alongside `object_type`).
    pub otype_raw: String,
    /// ICRS J2000 right ascension, decimal degrees.
    pub ra_deg: f64,
    /// ICRS J2000 declination, decimal degrees.
    pub dec_deg: f64,
    /// Provenance of the stored identity.
    pub source: TargetSource,
    /// RFC 3339 timestamp of the last seed/resolve/override.
    pub resolved_at: String,
    /// All aliases (designations + common names + user-added).
    pub aliases: Vec<ResolvedAlias>,
}

impl CachedTarget {
    /// Build a [`ResolvedIdentity`] view of this cached target.
    #[must_use]
    pub fn to_identity(&self) -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: self.simbad_oid,
            primary_designation: self.primary_designation.clone(),
            common_name: self.common_name.clone(),
            object_type: self.object_type,
            otype_raw: self.otype_raw.clone(),
            ra_deg: self.ra_deg,
            dec_deg: self.dec_deg,
            aliases: self.aliases.clone(),
            source: self.source,
        }
    }
}

/// A single ranked typeahead hit.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    /// The matched canonical target (aliases loaded).
    pub target: CachedTarget,
    /// The display form of the alias that matched.
    pub matched_alias: String,
    /// Rank bucket: `0` exact, `1` prefix, `2` substring.
    pub rank: u8,
}

/// Rank bucket for an exact normalized-alias match.
pub const RANK_EXACT: u8 = 0;
/// Rank bucket for a prefix match.
pub const RANK_PREFIX: u8 = 1;
/// Rank bucket for a substring match.
pub const RANK_SUBSTRING: u8 = 2;

/// Outcome of a [`Cache::upsert`] call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpsertOutcome {
    /// A new row was inserted.
    Inserted,
    /// An existing row (matched by oid or derived id) was updated.
    Updated,
    /// Skipped: an existing `user-override` row takes precedence.
    SkippedUserOverride,
}

/// State of a queued batch item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingState {
    /// Awaiting resolution (or a retry after a transient failure).
    Pending,
    /// Resolved to a canonical target.
    Resolved,
    /// A genuine content miss (unknown/ambiguous).
    Unresolved,
}

impl PendingState {
    /// The wire/DB string.
    #[must_use]
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Unresolved => "unresolved",
        }
    }

    /// Parse a wire/DB string; unknown → `None`.
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "resolved" => Some(Self::Resolved),
            "unresolved" => Some(Self::Unresolved),
            _ => None,
        }
    }
}

/// One queued batch item (read model for a [`Queue`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingItem {
    /// Opaque caller id (the queue key).
    pub id: String,
    /// Raw identifier to resolve.
    pub query: String,
    /// Current state.
    pub state: PendingState,
    /// Attempt counter (incremented only on content misses).
    pub attempts: i64,
    /// Resolved canonical-target id, when resolved.
    pub target_id: Option<Uuid>,
}

// ── Errors ───────────────────────────────────────────────────────────────────

/// Error type for [`Cache`] operations.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// Underlying backend failure (DB error, etc.).
    #[error("cache backend error: {0}")]
    Backend(String),
    /// A stored id was not a valid UUID.
    #[error("invalid stored uuid '{0}': {1}")]
    InvalidUuid(String, uuid::Error),
    /// A stored enum value was outside its closed set.
    #[error("invalid stored enum value: '{0}'")]
    InvalidEnum(String),
}

/// Error type for [`Queue`] operations.
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// Underlying backend failure.
    #[error("queue backend error: {0}")]
    Backend(String),
    /// A stored id was not a valid UUID.
    #[error("invalid stored uuid '{0}': {1}")]
    InvalidUuid(String, uuid::Error),
    /// A stored state value was outside its closed set.
    #[error("invalid stored state: '{0}'")]
    InvalidState(String),
}

// ── Cache trait ──────────────────────────────────────────────────────────────

/// The pluggable identity store.
///
/// Implementations MUST honour dedup + source precedence in [`Cache::upsert`]:
/// match an existing row by `simbad_oid` when `Some`, else by the caller's
/// designation-derived id (`simbad_resolver_core::identity::target_id_from_designation`);
/// a write proceeds iff `incoming.source.may_overwrite(existing.source)` (a
/// `user-override` row is sticky). Aliases are rewritten wholesale on update.
#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    /// Read a cached target by its persisted id.
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CachedTarget>, CacheError>;

    /// Read a cached target by its SIMBAD physical-object id.
    async fn get_by_simbad_oid(&self, oid: i64) -> Result<Option<CachedTarget>, CacheError>;

    /// Read a cached target by an exact normalized alias (normalize the query first).
    async fn get_by_normalized(&self, normalized: &str)
        -> Result<Option<CachedTarget>, CacheError>;

    /// Ranked typeahead search over aliases: exact > prefix > substring, deduped
    /// to one hit per target (best rank wins, ties → shortest alias), capped to
    /// `limit`. Local-only, no network. A blank query or `limit == 0` → empty.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, CacheError>;

    /// Upsert an identity (and its aliases) with dedup + precedence. `namespace`
    /// is the caller's id namespace for the designation-derived fallback id.
    async fn upsert(
        &self,
        identity: &ResolvedIdentity,
        namespace: &Uuid,
    ) -> Result<(Uuid, UpsertOutcome), CacheError>;

    /// Add a user alias (`kind = 'user'`). Returns `true` if newly inserted,
    /// `false` if it already existed (idempotent).
    async fn add_user_alias(&self, target_id: Uuid, alias: &str) -> Result<bool, CacheError>;

    /// Remove a user alias by id, only if its `kind = 'user'`. Returns whether a
    /// row was removed.
    async fn remove_user_alias(&self, alias_id: &str) -> Result<bool, CacheError>;

    /// List all cached targets (ordered by `primary_designation`).
    async fn list(&self) -> Result<Vec<CachedTarget>, CacheError>;
}

// ── Queue trait ──────────────────────────────────────────────────────────────

/// The pluggable pending-work store for the async batch resolver.
///
/// Transient failures (`ResolveError::is_transient`) → [`Queue::release`] (stay
/// pending, attempts unchanged). Content misses → [`Queue::mark_unresolved`]
/// (attempts += 1).
#[async_trait::async_trait]
pub trait Queue: Send + Sync {
    /// Enqueue an item (idempotent by `id`); a no-op if `id` already present.
    async fn enqueue(&self, id: &str, query: &str) -> Result<(), QueueError>;

    /// Claim up to `n` pending items for processing (approximately FIFO).
    async fn claim_pending(&self, n: usize) -> Result<Vec<PendingItem>, QueueError>;

    /// Mark an item resolved and bind its target (attempts unchanged).
    async fn mark_resolved(&self, id: &str, target_id: Uuid) -> Result<(), QueueError>;

    /// Mark an item unresolved (content miss); attempts += 1.
    async fn mark_unresolved(&self, id: &str) -> Result<(), QueueError>;

    /// Release a claimed item back to pending after a transient failure
    /// (attempts unchanged).
    async fn release(&self, id: &str) -> Result<(), QueueError>;

    /// Read a single item by id.
    async fn get(&self, id: &str) -> Result<Option<PendingItem>, QueueError>;

    /// Count items still `pending`.
    async fn pending_count(&self) -> Result<usize, QueueError>;
}
