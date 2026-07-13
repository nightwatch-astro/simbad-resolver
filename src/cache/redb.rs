//! Single redb-backed [`Cache`] + [`Queue`], durable (file) or ephemeral
//! (in-memory).
//!
//! # Table design
//!
//! redb keeps keys sorted; every table is keyed by a `&str` or `&str`-encoded
//! id and stores a `serde_json` blob, mirroring the in-memory backend's
//! indexes:
//!
//! - `targets` — `id (uuid) → StoredTarget` (the canonical row, aliases
//!   excluded). Dedup by `simbad_oid` and lookup-by-oid scan this table (the
//!   oid lives inside the row, so no separate oid index can go stale).
//! - `aliases` — `alias_id (uuid) → StoredAlias { target_id, alias }`, the
//!   single source of truth for every alias. `get_by_normalized`, `search`, and
//!   per-target assembly scan this table (a local typeahead cache is small, and
//!   a full scan matches the reference backend's behaviour exactly).
//! - `pending` — `id → StoredPending`, the batch queue; a monotonic `seq`
//!   (allocated from `meta`) gives `claim_pending` its approximate-FIFO order.
//! - `meta` — `&str → u64`; holds the next pending sequence number.
//!
//! redb is synchronous; the async trait methods run each unit of work inside
//! [`tokio::task::spawn_blocking`], moving a cloned `Arc<Database>` in.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    Cache, CacheError, CachedTarget, PendingItem, PendingState, Queue, QueueError, SearchHit,
    UpsertOutcome, RANK_EXACT, RANK_PREFIX, RANK_SUBSTRING,
};
use crate::identity::target_id_from_designation;
use crate::normalize::normalize;
use crate::types::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};

const TARGETS: TableDefinition<&str, &[u8]> = TableDefinition::new("targets");
const ALIASES: TableDefinition<&str, &[u8]> = TableDefinition::new("aliases");
const PENDING: TableDefinition<&str, &[u8]> = TableDefinition::new("pending");
const META: TableDefinition<&str, u64> = TableDefinition::new("meta");
const PENDING_SEQ_KEY: &str = "pending_seq";

/// Map any `Display` error (redb, serde, join) to a [`CacheError::Backend`].
fn cache_err<E: std::fmt::Display>(e: E) -> CacheError {
    CacheError::Backend(e.to_string())
}

/// Map any `Display` error (redb, serde, join) to a [`QueueError::Backend`].
fn queue_err<E: std::fmt::Display>(e: E) -> QueueError {
    QueueError::Backend(e.to_string())
}

/// The current time as an RFC 3339 timestamp (UTC).
fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("Rfc3339 format is always valid for OffsetDateTime")
}

// ── Persisted row structs ─────────────────────────────────────────────────────

/// A canonical target row (aliases excluded — those live in the `aliases`
/// table, keyed independently so `remove_user_alias` can address one).
#[derive(Serialize, Deserialize)]
struct StoredTarget {
    simbad_oid: Option<i64>,
    primary_designation: String,
    common_name: Option<String>,
    object_type: ObjectType,
    otype_raw: String,
    ra_deg: f64,
    dec_deg: f64,
    /// Johnson V magnitude when known. `#[serde(default)]` so target rows
    /// written before this field existed still deserialize (as `None`).
    #[serde(default)]
    v_mag: Option<f64>,
    source: TargetSource,
    resolved_at: String,
}

impl StoredTarget {
    fn from_identity(identity: &ResolvedIdentity) -> Self {
        Self {
            simbad_oid: identity.simbad_oid,
            primary_designation: identity.primary_designation.clone(),
            common_name: identity.common_name.clone(),
            object_type: identity.object_type,
            otype_raw: identity.otype_raw.clone(),
            ra_deg: identity.ra_deg,
            dec_deg: identity.dec_deg,
            v_mag: identity.v_mag,
            source: identity.source,
            resolved_at: now_iso(),
        }
    }

    fn into_target(self, id: Uuid, aliases: Vec<ResolvedAlias>) -> CachedTarget {
        CachedTarget {
            id,
            simbad_oid: self.simbad_oid,
            primary_designation: self.primary_designation,
            common_name: self.common_name,
            object_type: self.object_type,
            otype_raw: self.otype_raw,
            ra_deg: self.ra_deg,
            dec_deg: self.dec_deg,
            v_mag: self.v_mag,
            source: self.source,
            resolved_at: self.resolved_at,
            aliases,
        }
    }
}

/// One alias row: a [`ResolvedAlias`] owned by `target_id`.
#[derive(Serialize, Deserialize)]
struct StoredAlias {
    target_id: String,
    alias: ResolvedAlias,
}

/// One queued item. `state` is the [`PendingState`] wire string; `seq` is the
/// monotonic insertion order used to approximate FIFO in `claim_pending`.
#[derive(Serialize, Deserialize)]
struct StoredPending {
    query: String,
    state: String,
    attempts: i64,
    target_id: Option<String>,
    seq: u64,
}

impl StoredPending {
    fn into_item(self, id: String) -> Result<PendingItem, QueueError> {
        let state = PendingState::from_wire(&self.state)
            .ok_or_else(|| QueueError::InvalidState(self.state.clone()))?;
        let target_id = self
            .target_id
            .map(|t| Uuid::parse_str(&t).map_err(|e| QueueError::InvalidUuid(t.clone(), e)))
            .transpose()?;
        Ok(PendingItem { id, query: self.query, state, attempts: self.attempts, target_id })
    }
}

// ── Store ─────────────────────────────────────────────────────────────────────

/// An open redb database shared by a [`RedbCache`] and a [`RedbQueue`].
///
/// Cloning is cheap (an `Arc` handle over one `redb::Database`), so
/// [`Self::cache`] / [`Self::queue`] hand out independent wrappers sharing the
/// same underlying database.
///
/// [`Store::in_memory`] opens an ephemeral database, so this example is fully
/// runnable with no filesystem access. Share one [`Store`] between a
/// [`crate::SimbadResolver`] (via [`crate::CacheBackend::custom`]) and a
/// [`crate::BatchResolver`] to have both operate over the same rows.
///
/// ```
/// use simbad_resolver::Store;
///
/// # fn demo() -> Result<(), simbad_resolver::CacheError> {
/// let store = Store::in_memory()?;
/// let cache = store.cache(); // implements `Cache`
/// let queue = store.queue(); // implements `Queue`
/// # let _ = (cache, queue);
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Store {
    db: Arc<Database>,
}

impl Store {
    /// Open (creating if missing) a durable, file-backed store at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Backend`] if the database cannot be opened or its
    /// tables cannot be initialised.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CacheError> {
        let db = Database::create(path).map_err(cache_err)?;
        init_tables(&db)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Open a fresh, ephemeral in-memory store (nothing persisted to disk).
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::Backend`] if the in-memory database cannot be
    /// created or its tables cannot be initialised.
    pub fn in_memory() -> Result<Self, CacheError> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(cache_err)?;
        init_tables(&db)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// A [`RedbCache`] over this store's database.
    #[must_use]
    pub fn cache(&self) -> RedbCache {
        RedbCache { db: self.db.clone() }
    }

    /// A [`RedbQueue`] over this store's database.
    #[must_use]
    pub fn queue(&self) -> RedbQueue {
        RedbQueue { db: self.db.clone() }
    }
}

/// Create every table up front so later read transactions always find them
/// (redb errors on `open_table` for a table that was never written).
fn init_tables(db: &Database) -> Result<(), CacheError> {
    let w = db.begin_write().map_err(cache_err)?;
    {
        w.open_table(TARGETS).map_err(cache_err)?;
        w.open_table(ALIASES).map_err(cache_err)?;
        w.open_table(PENDING).map_err(cache_err)?;
        w.open_table(META).map_err(cache_err)?;
    }
    w.commit().map_err(cache_err)?;
    Ok(())
}

/// The durable/ephemeral redb-backed [`Cache`].
#[derive(Clone)]
pub struct RedbCache {
    db: Arc<Database>,
}

/// The durable/ephemeral redb-backed [`Queue`].
#[derive(Clone)]
pub struct RedbQueue {
    db: Arc<Database>,
}

// ── Cache sync operations ─────────────────────────────────────────────────────

/// Load every alias owned by `target_id`, sorted by display form.
fn aliases_for(db: &Database, target_id: &str) -> Result<Vec<ResolvedAlias>, CacheError> {
    let txn = db.begin_read().map_err(cache_err)?;
    let table = txn.open_table(ALIASES).map_err(cache_err)?;
    let mut aliases = Vec::new();
    for entry in table.iter().map_err(cache_err)? {
        let (_, v) = entry.map_err(cache_err)?;
        let sa: StoredAlias = serde_json::from_slice(v.value()).map_err(cache_err)?;
        if sa.target_id == target_id {
            aliases.push(sa.alias);
        }
    }
    aliases.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(aliases)
}

fn get_by_id(db: &Database, id: Uuid) -> Result<Option<CachedTarget>, CacheError> {
    let id_str = id.to_string();
    let txn = db.begin_read().map_err(cache_err)?;
    let targets = txn.open_table(TARGETS).map_err(cache_err)?;
    let Some(bytes) = targets.get(id_str.as_str()).map_err(cache_err)? else {
        return Ok(None);
    };
    let st: StoredTarget = serde_json::from_slice(bytes.value()).map_err(cache_err)?;
    drop(bytes);
    let aliases = aliases_for(db, &id_str)?;
    Ok(Some(st.into_target(id, aliases)))
}

fn get_by_simbad_oid(db: &Database, oid: i64) -> Result<Option<CachedTarget>, CacheError> {
    let found = {
        let txn = db.begin_read().map_err(cache_err)?;
        let targets = txn.open_table(TARGETS).map_err(cache_err)?;
        let mut hit: Option<String> = None;
        for entry in targets.iter().map_err(cache_err)? {
            let (k, v) = entry.map_err(cache_err)?;
            let st: StoredTarget = serde_json::from_slice(v.value()).map_err(cache_err)?;
            if st.simbad_oid == Some(oid) {
                hit = Some(k.value().to_string());
                break;
            }
        }
        hit
    };
    match found {
        None => Ok(None),
        Some(id_str) => {
            let id = Uuid::parse_str(&id_str).map_err(|e| CacheError::InvalidUuid(id_str, e))?;
            get_by_id(db, id)
        }
    }
}

fn get_by_normalized(db: &Database, normalized: &str) -> Result<Option<CachedTarget>, CacheError> {
    let found = {
        let txn = db.begin_read().map_err(cache_err)?;
        let table = txn.open_table(ALIASES).map_err(cache_err)?;
        let mut hit: Option<String> = None;
        for entry in table.iter().map_err(cache_err)? {
            let (_, v) = entry.map_err(cache_err)?;
            let sa: StoredAlias = serde_json::from_slice(v.value()).map_err(cache_err)?;
            if sa.alias.normalized == normalized {
                hit = Some(sa.target_id);
                break;
            }
        }
        hit
    };
    match found {
        None => Ok(None),
        Some(id_str) => {
            let id = Uuid::parse_str(&id_str).map_err(|e| CacheError::InvalidUuid(id_str, e))?;
            get_by_id(db, id)
        }
    }
}

/// The best-ranked alias hit seen so far for one target during search dedup.
struct Best {
    alias: String,
    normalized_len: usize,
    rank: u8,
}

impl Best {
    /// A lower rank wins; ties break on the shorter matched alias.
    fn is_better_than(&self, other: &Self) -> bool {
        (self.rank, self.normalized_len) < (other.rank, other.normalized_len)
    }
}

fn search(db: &Database, query: &str, limit: usize) -> Result<Vec<SearchHit>, CacheError> {
    let q = normalize(query);
    if q.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let mut best_by_target: HashMap<String, Best> = HashMap::new();
    {
        let txn = db.begin_read().map_err(cache_err)?;
        let table = txn.open_table(ALIASES).map_err(cache_err)?;
        for entry in table.iter().map_err(cache_err)? {
            let (_, v) = entry.map_err(cache_err)?;
            let sa: StoredAlias = serde_json::from_slice(v.value()).map_err(cache_err)?;
            let normalized = &sa.alias.normalized;
            let rank = if *normalized == q {
                RANK_EXACT
            } else if normalized.starts_with(&q) {
                RANK_PREFIX
            } else if normalized.contains(&q) {
                RANK_SUBSTRING
            } else {
                continue;
            };
            let candidate =
                Best { alias: sa.alias.alias.clone(), normalized_len: normalized.len(), rank };
            match best_by_target.entry(sa.target_id) {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if candidate.is_better_than(e.get()) {
                        e.insert(candidate);
                    }
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(candidate);
                }
            }
        }
    }

    let mut ranked: Vec<(String, Best)> = best_by_target.into_iter().collect();
    ranked.sort_by(|(_, a), (_, b)| {
        (a.rank, a.normalized_len, a.alias.as_str()).cmp(&(
            b.rank,
            b.normalized_len,
            b.alias.as_str(),
        ))
    });
    ranked.truncate(limit);

    let mut hits = Vec::with_capacity(ranked.len());
    for (target_id, best) in ranked {
        let id = Uuid::parse_str(&target_id).map_err(|e| CacheError::InvalidUuid(target_id, e))?;
        if let Some(target) = get_by_id(db, id)? {
            hits.push(SearchHit { target, matched_alias: best.alias, rank: best.rank });
        }
    }
    Ok(hits)
}

/// Replace all alias rows for `target_id` wholesale, ensuring the primary
/// designation is present as a `designation` alias and deduping by normalized
/// form (mirrors the in-memory backend's `rewrite_aliases`).
fn rewrite_aliases(
    w: &redb::WriteTransaction,
    target_id: &str,
    identity: &ResolvedIdentity,
) -> Result<(), CacheError> {
    let mut table = w.open_table(ALIASES).map_err(cache_err)?;

    let stale: Vec<String> = {
        let mut keys = Vec::new();
        for entry in table.iter().map_err(cache_err)? {
            let (k, v) = entry.map_err(cache_err)?;
            let sa: StoredAlias = serde_json::from_slice(v.value()).map_err(cache_err)?;
            if sa.target_id == target_id {
                keys.push(k.value().to_string());
            }
        }
        keys
    };
    for k in stale {
        table.remove(k.as_str()).map_err(cache_err)?;
    }

    let mut aliases = identity.aliases.clone();
    let primary_norm = normalize(&identity.primary_designation);
    if !aliases.iter().any(|a| a.normalized == primary_norm) {
        aliases
            .push(ResolvedAlias::new(identity.primary_designation.clone(), AliasKind::Designation));
    }

    let mut seen = HashSet::with_capacity(aliases.len());
    for alias in aliases {
        if !seen.insert(alias.normalized.clone()) {
            continue; // tolerate duplicate normalized forms within one identity
        }
        let alias_id = Uuid::new_v4().to_string();
        let sa = StoredAlias { target_id: target_id.to_owned(), alias };
        let bytes = serde_json::to_vec(&sa).map_err(cache_err)?;
        table.insert(alias_id.as_str(), bytes.as_slice()).map_err(cache_err)?;
    }
    Ok(())
}

fn upsert(
    db: &Database,
    identity: &ResolvedIdentity,
    namespace: &Uuid,
) -> Result<(Uuid, UpsertOutcome), CacheError> {
    let derived_id =
        target_id_from_designation(namespace, &identity.primary_designation).to_string();
    let w = db.begin_write().map_err(cache_err)?;

    let (id_str, outcome, wrote_row) = {
        let mut targets = w.open_table(TARGETS).map_err(cache_err)?;

        // Find the row to upsert into: by oid when Some, else by derived id.
        let mut existing: Option<(String, TargetSource)> = None;
        if let Some(oid) = identity.simbad_oid {
            for entry in targets.iter().map_err(cache_err)? {
                let (k, v) = entry.map_err(cache_err)?;
                let st: StoredTarget = serde_json::from_slice(v.value()).map_err(cache_err)?;
                if st.simbad_oid == Some(oid) {
                    existing = Some((k.value().to_string(), st.source));
                    break;
                }
            }
        }
        if existing.is_none() {
            if let Some(v) = targets.get(derived_id.as_str()).map_err(cache_err)? {
                let st: StoredTarget = serde_json::from_slice(v.value()).map_err(cache_err)?;
                existing = Some((derived_id.clone(), st.source));
            }
        }

        match existing {
            Some((id, source)) if !identity.source.may_overwrite(source) => {
                (id, UpsertOutcome::SkippedUserOverride, false)
            }
            Some((id, _)) => {
                let bytes = serde_json::to_vec(&StoredTarget::from_identity(identity))
                    .map_err(cache_err)?;
                targets.insert(id.as_str(), bytes.as_slice()).map_err(cache_err)?;
                (id, UpsertOutcome::Updated, true)
            }
            None => {
                let bytes = serde_json::to_vec(&StoredTarget::from_identity(identity))
                    .map_err(cache_err)?;
                targets.insert(derived_id.as_str(), bytes.as_slice()).map_err(cache_err)?;
                (derived_id.clone(), UpsertOutcome::Inserted, true)
            }
        }
    };

    if wrote_row {
        rewrite_aliases(&w, &id_str, identity)?;
    }
    w.commit().map_err(cache_err)?;

    let id = Uuid::parse_str(&id_str).map_err(|e| CacheError::InvalidUuid(id_str, e))?;
    Ok((id, outcome))
}

fn add_user_alias(db: &Database, target_id: Uuid, alias: &str) -> Result<bool, CacheError> {
    let normalized = normalize(alias);
    if normalized.is_empty() {
        return Ok(false);
    }
    let tid = target_id.to_string();
    let w = db.begin_write().map_err(cache_err)?;
    let inserted = {
        let mut table = w.open_table(ALIASES).map_err(cache_err)?;
        let exists = {
            let mut found = false;
            for entry in table.iter().map_err(cache_err)? {
                let (_, v) = entry.map_err(cache_err)?;
                let sa: StoredAlias = serde_json::from_slice(v.value()).map_err(cache_err)?;
                if sa.target_id == tid && sa.alias.normalized == normalized {
                    found = true;
                    break;
                }
            }
            found
        };
        if exists {
            false
        } else {
            let alias_id = Uuid::new_v4().to_string();
            let sa = StoredAlias {
                target_id: tid.clone(),
                alias: ResolvedAlias::new(alias, AliasKind::User),
            };
            let bytes = serde_json::to_vec(&sa).map_err(cache_err)?;
            table.insert(alias_id.as_str(), bytes.as_slice()).map_err(cache_err)?;
            true
        }
    };
    w.commit().map_err(cache_err)?;
    Ok(inserted)
}

fn remove_user_alias(db: &Database, alias_id: &str) -> Result<bool, CacheError> {
    let w = db.begin_write().map_err(cache_err)?;
    let removed = {
        let mut table = w.open_table(ALIASES).map_err(cache_err)?;
        // Copy the row out so the read guard is dropped before the write borrow.
        let existing = table.get(alias_id).map_err(cache_err)?.map(|v| v.value().to_vec());
        match existing {
            None => false,
            Some(raw) => {
                let sa: StoredAlias = serde_json::from_slice(&raw).map_err(cache_err)?;
                if sa.alias.kind == AliasKind::User {
                    table.remove(alias_id).map_err(cache_err)?;
                    true
                } else {
                    false
                }
            }
        }
    };
    w.commit().map_err(cache_err)?;
    Ok(removed)
}

fn list(db: &Database) -> Result<Vec<CachedTarget>, CacheError> {
    let txn = db.begin_read().map_err(cache_err)?;
    let aliases_tbl = txn.open_table(ALIASES).map_err(cache_err)?;
    let mut aliases_by: HashMap<String, Vec<ResolvedAlias>> = HashMap::new();
    for entry in aliases_tbl.iter().map_err(cache_err)? {
        let (_, v) = entry.map_err(cache_err)?;
        let sa: StoredAlias = serde_json::from_slice(v.value()).map_err(cache_err)?;
        aliases_by.entry(sa.target_id).or_default().push(sa.alias);
    }

    let targets = txn.open_table(TARGETS).map_err(cache_err)?;
    let mut out = Vec::new();
    for entry in targets.iter().map_err(cache_err)? {
        let (k, v) = entry.map_err(cache_err)?;
        let id_str = k.value().to_string();
        let st: StoredTarget = serde_json::from_slice(v.value()).map_err(cache_err)?;
        let id =
            Uuid::parse_str(&id_str).map_err(|e| CacheError::InvalidUuid(id_str.clone(), e))?;
        let mut aliases = aliases_by.remove(&id_str).unwrap_or_default();
        aliases.sort_by(|a, b| a.alias.cmp(&b.alias));
        out.push(st.into_target(id, aliases));
    }
    out.sort_by(|a, b| a.primary_designation.cmp(&b.primary_designation));
    Ok(out)
}

#[async_trait::async_trait]
impl Cache for RedbCache {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CachedTarget>, CacheError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || get_by_id(&db, id)).await.map_err(cache_err)?
    }

    async fn get_by_simbad_oid(&self, oid: i64) -> Result<Option<CachedTarget>, CacheError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || get_by_simbad_oid(&db, oid)).await.map_err(cache_err)?
    }

    async fn get_by_normalized(
        &self,
        normalized: &str,
    ) -> Result<Option<CachedTarget>, CacheError> {
        let db = self.db.clone();
        let normalized = normalized.to_owned();
        tokio::task::spawn_blocking(move || get_by_normalized(&db, &normalized))
            .await
            .map_err(cache_err)?
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, CacheError> {
        let db = self.db.clone();
        let query = query.to_owned();
        tokio::task::spawn_blocking(move || search(&db, &query, limit)).await.map_err(cache_err)?
    }

    async fn upsert(
        &self,
        identity: &ResolvedIdentity,
        namespace: &Uuid,
    ) -> Result<(Uuid, UpsertOutcome), CacheError> {
        let db = self.db.clone();
        let identity = identity.clone();
        let namespace = *namespace;
        tokio::task::spawn_blocking(move || upsert(&db, &identity, &namespace))
            .await
            .map_err(cache_err)?
    }

    async fn add_user_alias(&self, target_id: Uuid, alias: &str) -> Result<bool, CacheError> {
        let db = self.db.clone();
        let alias = alias.to_owned();
        tokio::task::spawn_blocking(move || add_user_alias(&db, target_id, &alias))
            .await
            .map_err(cache_err)?
    }

    async fn remove_user_alias(&self, alias_id: &str) -> Result<bool, CacheError> {
        let db = self.db.clone();
        let alias_id = alias_id.to_owned();
        tokio::task::spawn_blocking(move || remove_user_alias(&db, &alias_id))
            .await
            .map_err(cache_err)?
    }

    async fn list(&self) -> Result<Vec<CachedTarget>, CacheError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || list(&db)).await.map_err(cache_err)?
    }
}

// ── Queue sync operations ─────────────────────────────────────────────────────

fn enqueue(db: &Database, id: &str, query: &str) -> Result<(), QueueError> {
    let w = db.begin_write().map_err(queue_err)?;
    {
        let mut pending = w.open_table(PENDING).map_err(queue_err)?;
        if pending.get(id).map_err(queue_err)?.is_none() {
            let mut meta = w.open_table(META).map_err(queue_err)?;
            let seq = meta.get(PENDING_SEQ_KEY).map_err(queue_err)?.map_or(0, |g| g.value());
            meta.insert(PENDING_SEQ_KEY, seq + 1).map_err(queue_err)?;
            let sp = StoredPending {
                query: query.to_owned(),
                state: PendingState::Pending.as_wire().to_owned(),
                attempts: 0,
                target_id: None,
                seq,
            };
            let bytes = serde_json::to_vec(&sp).map_err(queue_err)?;
            pending.insert(id, bytes.as_slice()).map_err(queue_err)?;
        }
    }
    w.commit().map_err(queue_err)?;
    Ok(())
}

fn claim_pending(db: &Database, n: usize) -> Result<Vec<PendingItem>, QueueError> {
    let txn = db.begin_read().map_err(queue_err)?;
    let pending = txn.open_table(PENDING).map_err(queue_err)?;
    let mut rows: Vec<(String, StoredPending)> = Vec::new();
    for entry in pending.iter().map_err(queue_err)? {
        let (k, v) = entry.map_err(queue_err)?;
        let sp: StoredPending = serde_json::from_slice(v.value()).map_err(queue_err)?;
        if sp.state == PendingState::Pending.as_wire() {
            rows.push((k.value().to_string(), sp));
        }
    }
    rows.sort_by_key(|(_, sp)| sp.seq);
    rows.truncate(n);
    rows.into_iter().map(|(id, sp)| sp.into_item(id)).collect()
}

/// Read-modify-write one pending row (a no-op if the id is absent).
fn update_pending(
    db: &Database,
    id: &str,
    mutate: impl FnOnce(&mut StoredPending),
) -> Result<(), QueueError> {
    let w = db.begin_write().map_err(queue_err)?;
    {
        let mut pending = w.open_table(PENDING).map_err(queue_err)?;
        // Copy the row out so the read guard is dropped before the write borrow.
        let existing = pending.get(id).map_err(queue_err)?.map(|v| v.value().to_vec());
        if let Some(raw) = existing {
            let mut sp: StoredPending = serde_json::from_slice(&raw).map_err(queue_err)?;
            mutate(&mut sp);
            let bytes = serde_json::to_vec(&sp).map_err(queue_err)?;
            pending.insert(id, bytes.as_slice()).map_err(queue_err)?;
        }
    }
    w.commit().map_err(queue_err)?;
    Ok(())
}

fn get(db: &Database, id: &str) -> Result<Option<PendingItem>, QueueError> {
    let txn = db.begin_read().map_err(queue_err)?;
    let pending = txn.open_table(PENDING).map_err(queue_err)?;
    match pending.get(id).map_err(queue_err)? {
        None => Ok(None),
        Some(v) => {
            let sp: StoredPending = serde_json::from_slice(v.value()).map_err(queue_err)?;
            Ok(Some(sp.into_item(id.to_owned())?))
        }
    }
}

fn pending_count(db: &Database) -> Result<usize, QueueError> {
    let txn = db.begin_read().map_err(queue_err)?;
    let pending = txn.open_table(PENDING).map_err(queue_err)?;
    let mut count = 0usize;
    for entry in pending.iter().map_err(queue_err)? {
        let (_, v) = entry.map_err(queue_err)?;
        let sp: StoredPending = serde_json::from_slice(v.value()).map_err(queue_err)?;
        if sp.state == PendingState::Pending.as_wire() {
            count += 1;
        }
    }
    Ok(count)
}

#[async_trait::async_trait]
impl Queue for RedbQueue {
    async fn enqueue(&self, id: &str, query: &str) -> Result<(), QueueError> {
        let db = self.db.clone();
        let (id, query) = (id.to_owned(), query.to_owned());
        tokio::task::spawn_blocking(move || enqueue(&db, &id, &query)).await.map_err(queue_err)?
    }

    async fn claim_pending(&self, n: usize) -> Result<Vec<PendingItem>, QueueError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || claim_pending(&db, n)).await.map_err(queue_err)?
    }

    async fn mark_resolved(&self, id: &str, target_id: Uuid) -> Result<(), QueueError> {
        let db = self.db.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            update_pending(&db, &id, |sp| {
                PendingState::Resolved.as_wire().clone_into(&mut sp.state);
                sp.target_id = Some(target_id.to_string());
            })
        })
        .await
        .map_err(queue_err)?
    }

    async fn mark_unresolved(&self, id: &str) -> Result<(), QueueError> {
        let db = self.db.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            update_pending(&db, &id, |sp| {
                PendingState::Unresolved.as_wire().clone_into(&mut sp.state);
                sp.attempts += 1;
            })
        })
        .await
        .map_err(queue_err)?
    }

    async fn release(&self, id: &str) -> Result<(), QueueError> {
        let db = self.db.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            update_pending(&db, &id, |sp| {
                PendingState::Pending.as_wire().clone_into(&mut sp.state);
            })
        })
        .await
        .map_err(queue_err)?
    }

    async fn get(&self, id: &str) -> Result<Option<PendingItem>, QueueError> {
        let db = self.db.clone();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || get(&db, &id)).await.map_err(queue_err)?
    }

    async fn pending_count(&self) -> Result<usize, QueueError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || pending_count(&db)).await.map_err(queue_err)?
    }
}

#[cfg(test)]
mod tests {
    //! Covers `remove_user_alias`, whose real-removal path needs an opaque
    //! alias id the public API never exposes — the dashmap backend tested this
    //! via internals too, so it lives here rather than in `tests/cache.rs`.
    use super::*;

    fn m31() -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            otype_raw: "G".to_owned(),
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            v_mag: Some(3.44),
            aliases: vec![
                ResolvedAlias::new("M 31", AliasKind::Designation),
                ResolvedAlias::new("NGC 224", AliasKind::Designation),
                ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
            ],
            source: TargetSource::Resolved,
        }
    }

    /// Recover an opaque alias id by its display form (test-only introspection).
    fn alias_id_of(db: &Database, display: &str) -> String {
        let txn = db.begin_read().unwrap();
        let table = txn.open_table(ALIASES).unwrap();
        for entry in table.iter().unwrap() {
            let (k, v) = entry.unwrap();
            let sa: StoredAlias = serde_json::from_slice(v.value()).unwrap();
            if sa.alias.alias == display {
                return k.value().to_string();
            }
        }
        panic!("alias {display:?} not found");
    }

    #[tokio::test]
    async fn remove_user_alias_only_removes_user_kind() {
        let store = Store::in_memory().unwrap();
        let cache = store.cache();
        let ns = crate::identity::namespace("redb.tests");
        let (id, _) = cache.upsert(&m31(), &ns).await.unwrap();
        cache.add_user_alias(id, "M31 favorite").await.unwrap();

        let user_alias_id = alias_id_of(&store.db, "M31 favorite");
        assert!(cache.remove_user_alias(&user_alias_id).await.unwrap());
        assert_eq!(cache.get_by_id(id).await.unwrap().unwrap().aliases.len(), 3);

        // A designation alias's id must not be removable via remove_user_alias.
        let designation_id = alias_id_of(&store.db, "M 31");
        assert!(!cache.remove_user_alias(&designation_id).await.unwrap());
        assert_eq!(cache.get_by_id(id).await.unwrap().unwrap().aliases.len(), 3);
    }
}
