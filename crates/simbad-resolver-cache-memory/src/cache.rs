//! [`MemoryCache`]: an in-memory `Cache` impl backed by `dashmap`.
//!
//! Mirrors the dedup/precedence and ranked-search semantics of astro-plan's
//! SQLite `targeting::resolver::cache` (the reference impl this crate's test
//! suite is ported from), adapted from SQL tables to sharded maps:
//! - `targets`: `id -> TargetRow` (the canonical row, aliases excluded).
//! - `aliases`: `alias_id -> AliasRow` (mirrors the `target_alias` table; the
//!   single source of truth for every alias, joined at read time).
//! - `by_oid` / `by_normalized`: secondary indices for O(1) lookup, kept in
//!   sync on every alias/row mutation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use simbad_resolver_cache::{
    Cache, CacheError, CachedTarget, SearchHit, UpsertOutcome, RANK_EXACT, RANK_PREFIX,
    RANK_SUBSTRING,
};
use simbad_resolver_core::identity::target_id_from_designation;
use simbad_resolver_core::normalize::normalize;
use simbad_resolver_core::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};
use uuid::Uuid;

/// The canonical row for one target, without its aliases (those live in
/// [`Inner::aliases`], joined at read time via [`MemoryCache::assemble`]).
#[derive(Clone)]
struct TargetRow {
    id: Uuid,
    simbad_oid: Option<i64>,
    primary_designation: String,
    common_name: Option<String>,
    object_type: ObjectType,
    otype_raw: String,
    ra_deg: f64,
    dec_deg: f64,
    source: TargetSource,
    resolved_at: String,
}

/// One alias row: an [`ResolvedAlias`] owned by `target_id`, keyed by a
/// stable `alias_id` so [`Cache::remove_user_alias`] can address it.
#[derive(Clone)]
struct AliasRow {
    target_id: Uuid,
    alias: ResolvedAlias,
}

#[derive(Default)]
struct Inner {
    targets: DashMap<Uuid, TargetRow>,
    by_oid: DashMap<i64, Uuid>,
    by_normalized: DashMap<String, Uuid>,
    aliases: DashMap<String, AliasRow>,
}

/// In-memory, `dashmap`-backed [`Cache`] implementation.
///
/// Cheaply `Clone`-able (an `Arc` handle over shared shard maps); every clone
/// observes the same underlying store.
#[derive(Clone, Default)]
pub struct MemoryCache(Arc<Inner>);

impl MemoryCache {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Join a target's canonical row with its current alias set.
    fn assemble(&self, id: Uuid) -> Option<CachedTarget> {
        let row = self.0.targets.get(&id)?;
        let mut aliases: Vec<ResolvedAlias> = self
            .0
            .aliases
            .iter()
            .filter(|entry| entry.value().target_id == id)
            .map(|entry| entry.value().alias.clone())
            .collect();
        aliases.sort_by(|a, b| a.alias.cmp(&b.alias));
        Some(CachedTarget {
            id: row.id,
            simbad_oid: row.simbad_oid,
            primary_designation: row.primary_designation.clone(),
            common_name: row.common_name.clone(),
            object_type: row.object_type,
            otype_raw: row.otype_raw.clone(),
            ra_deg: row.ra_deg,
            dec_deg: row.dec_deg,
            source: row.source,
            resolved_at: row.resolved_at.clone(),
            aliases,
        })
    }

    /// Find the row an incoming identity should upsert into: by `simbad_oid`
    /// when `Some`, else by the designation-derived `derived` id.
    fn find_existing(
        &self,
        identity: &ResolvedIdentity,
        derived: Uuid,
    ) -> Option<(Uuid, TargetSource)> {
        if let Some(oid) = identity.simbad_oid {
            let by_oid = self.0.by_oid.get(&oid).map(|entry| *entry);
            if let Some(id) = by_oid {
                if let Some(row) = self.0.targets.get(&id) {
                    return Some((id, row.source));
                }
            }
        }
        self.0.targets.get(&derived).map(|row| (derived, row.source))
    }

    /// Replace all alias rows for `target_id` wholesale, ensuring the primary
    /// designation is present as a `designation` alias even if the caller
    /// omitted it from `identity.aliases`.
    fn rewrite_aliases(&self, target_id: Uuid, identity: &ResolvedIdentity) {
        let stale: Vec<String> = self
            .0
            .aliases
            .iter()
            .filter(|entry| entry.value().target_id == target_id)
            .map(|entry| entry.key().clone())
            .collect();
        for alias_id in stale {
            if let Some((_, row)) = self.0.aliases.remove(&alias_id) {
                self.reindex_after_removal(&row.alias.normalized, target_id);
            }
        }

        let mut aliases = identity.aliases.clone();
        let primary_norm = normalize(&identity.primary_designation);
        if !aliases.iter().any(|a| a.normalized == primary_norm) {
            aliases.push(ResolvedAlias::new(
                identity.primary_designation.clone(),
                AliasKind::Designation,
            ));
        }

        let mut seen = HashSet::with_capacity(aliases.len());
        for alias in aliases {
            if !seen.insert(alias.normalized.clone()) {
                continue; // tolerate duplicate normalized forms within one identity
            }
            let alias_id = Uuid::new_v4().to_string();
            self.0.by_normalized.insert(alias.normalized.clone(), target_id);
            self.0.aliases.insert(alias_id, AliasRow { target_id, alias });
        }
    }

    /// After removing the alias row that produced `normalized`, either hand
    /// the `by_normalized` index to another surviving row with the same
    /// normalized text, or drop the index entry if it still points at the
    /// (now alias-less) `removed_target`.
    fn reindex_after_removal(&self, normalized: &str, removed_target: Uuid) {
        let replacement = self
            .0
            .aliases
            .iter()
            .find(|entry| entry.value().alias.normalized == normalized)
            .map(|entry| entry.value().target_id);
        if let Some(replacement) = replacement {
            self.0.by_normalized.insert(normalized.to_owned(), replacement);
        } else {
            self.0.by_normalized.remove_if(normalized, |_, v| *v == removed_target);
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

#[async_trait::async_trait]
impl Cache for MemoryCache {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CachedTarget>, CacheError> {
        Ok(self.assemble(id))
    }

    async fn get_by_simbad_oid(&self, oid: i64) -> Result<Option<CachedTarget>, CacheError> {
        let id = self.0.by_oid.get(&oid).map(|entry| *entry);
        Ok(id.and_then(|id| self.assemble(id)))
    }

    async fn get_by_normalized(
        &self,
        normalized: &str,
    ) -> Result<Option<CachedTarget>, CacheError> {
        let id = self.0.by_normalized.get(normalized).map(|entry| *entry);
        Ok(id.and_then(|id| self.assemble(id)))
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, CacheError> {
        let q = normalize(query);
        if q.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let mut best_by_target: HashMap<Uuid, Best> = HashMap::new();
        for entry in &self.0.aliases {
            let row = entry.value();
            let normalized = &row.alias.normalized;
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
                Best { alias: row.alias.alias.clone(), normalized_len: normalized.len(), rank };
            match best_by_target.entry(row.target_id) {
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

        let mut ranked: Vec<(Uuid, Best)> = best_by_target.into_iter().collect();
        ranked.sort_by(|(_, a), (_, b)| {
            (a.rank, a.normalized_len, a.alias.as_str()).cmp(&(
                b.rank,
                b.normalized_len,
                b.alias.as_str(),
            ))
        });
        ranked.truncate(limit);

        Ok(ranked
            .into_iter()
            .filter_map(|(target_id, best)| {
                self.assemble(target_id).map(|target| SearchHit {
                    target,
                    matched_alias: best.alias,
                    rank: best.rank,
                })
            })
            .collect())
    }

    async fn upsert(
        &self,
        identity: &ResolvedIdentity,
        namespace: &Uuid,
    ) -> Result<(Uuid, UpsertOutcome), CacheError> {
        let derived = target_id_from_designation(namespace, &identity.primary_designation);
        let existing = self.find_existing(identity, derived);

        let (id, outcome) = match existing {
            Some((id, source)) if !identity.source.may_overwrite(source) => {
                return Ok((id, UpsertOutcome::SkippedUserOverride));
            }
            Some((id, _)) => (id, UpsertOutcome::Updated),
            None => (derived, UpsertOutcome::Inserted),
        };

        let previous_oid = self.0.targets.get(&id).map(|row| row.simbad_oid);
        self.0.targets.insert(
            id,
            TargetRow {
                id,
                simbad_oid: identity.simbad_oid,
                primary_designation: identity.primary_designation.clone(),
                common_name: identity.common_name.clone(),
                object_type: identity.object_type,
                otype_raw: identity.otype_raw.clone(),
                ra_deg: identity.ra_deg,
                dec_deg: identity.dec_deg,
                source: identity.source,
                resolved_at: now_iso(),
            },
        );

        if let Some(Some(prev_oid)) = previous_oid {
            if Some(prev_oid) != identity.simbad_oid {
                self.0.by_oid.remove_if(&prev_oid, |_, v| *v == id);
            }
        }
        if let Some(new_oid) = identity.simbad_oid {
            self.0.by_oid.insert(new_oid, id);
        }

        self.rewrite_aliases(id, identity);

        Ok((id, outcome))
    }

    async fn add_user_alias(&self, target_id: Uuid, alias: &str) -> Result<bool, CacheError> {
        let normalized = normalize(alias);
        if normalized.is_empty() {
            return Ok(false);
        }
        let exists =
            self.0.aliases.iter().any(|e| {
                e.value().target_id == target_id && e.value().alias.normalized == normalized
            });
        if exists {
            return Ok(false);
        }
        let alias_id = Uuid::new_v4().to_string();
        self.0.by_normalized.insert(normalized, target_id);
        self.0.aliases.insert(
            alias_id,
            AliasRow { target_id, alias: ResolvedAlias::new(alias, AliasKind::User) },
        );
        Ok(true)
    }

    async fn remove_user_alias(&self, alias_id: &str) -> Result<bool, CacheError> {
        let Some(row) = self.0.aliases.get(alias_id) else {
            return Ok(false);
        };
        if row.alias.kind != AliasKind::User {
            return Ok(false);
        }
        let target_id = row.target_id;
        let normalized = row.alias.normalized.clone();
        drop(row);

        self.0.aliases.remove(alias_id);
        self.reindex_after_removal(&normalized, target_id);
        Ok(true)
    }

    async fn list(&self) -> Result<Vec<CachedTarget>, CacheError> {
        let ids: Vec<Uuid> = self.0.targets.iter().map(|entry| *entry.key()).collect();
        let mut items: Vec<CachedTarget> =
            ids.into_iter().filter_map(|id| self.assemble(id)).collect();
        items.sort_by(|a, b| a.primary_designation.cmp(&b.primary_designation));
        Ok(items)
    }
}

/// The current time as an RFC 3339 timestamp (UTC).
fn now_iso() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("Rfc3339 format is always valid for OffsetDateTime")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns() -> Uuid {
        simbad_resolver_core::identity::namespace("simbad-resolver-cache-memory.tests")
    }

    fn m31(source: TargetSource) -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(1_575_544),
            primary_designation: "M 31".to_owned(),
            common_name: Some("Andromeda Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            otype_raw: "G".to_owned(),
            ra_deg: 10.684_708,
            dec_deg: 41.268_75,
            aliases: vec![
                ResolvedAlias::new("M 31", AliasKind::Designation),
                ResolvedAlias::new("NGC 224", AliasKind::Designation),
                ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
            ],
            source,
        }
    }

    fn m101() -> ResolvedIdentity {
        ResolvedIdentity {
            simbad_oid: Some(3_456_789),
            primary_designation: "M 101".to_owned(),
            common_name: Some("Pinwheel Galaxy".to_owned()),
            object_type: ObjectType::Galaxy,
            otype_raw: "G".to_owned(),
            ra_deg: 210.802_42,
            dec_deg: 54.348_95,
            aliases: vec![
                ResolvedAlias::new("M 101", AliasKind::Designation),
                ResolvedAlias::new("NGC 5457", AliasKind::Designation),
                ResolvedAlias::new("Pinwheel Galaxy", AliasKind::CommonName),
            ],
            source: TargetSource::Seed,
        }
    }

    async fn seeded(cache: &MemoryCache) {
        cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();
        cache.upsert(&m101(), &ns()).await.unwrap();
    }

    // ── upsert dedup / precedence ────────────────────────────────────────────

    #[tokio::test]
    async fn insert_then_read_by_oid() {
        let cache = MemoryCache::new();
        let (id, outcome) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Inserted);

        let got = cache.get_by_simbad_oid(1_575_544).await.unwrap().unwrap();
        assert_eq!(got.id, id);
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.object_type, ObjectType::Galaxy);
        assert_eq!(got.source, TargetSource::Resolved);
        assert_eq!(got.aliases.len(), 3);
    }

    #[tokio::test]
    async fn read_by_normalized_alias() {
        let cache = MemoryCache::new();
        cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

        let got = cache.get_by_normalized(&normalize("NGC 224")).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");

        let got2 = cache.get_by_normalized(&normalize("Andromeda Galaxy")).await.unwrap().unwrap();
        assert_eq!(got2.id, got.id);
    }

    #[tokio::test]
    async fn dedup_by_oid_updates_single_row() {
        let cache = MemoryCache::new();
        let (id1, _) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

        let mut alt = m31(TargetSource::Resolved);
        alt.primary_designation = "NGC 224".to_owned();
        let (id2, outcome) = cache.upsert(&alt, &ns()).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Updated);
        assert_eq!(id1, id2, "dedup by oid must keep the same row id");

        assert_eq!(cache.list().await.unwrap().len(), 1);
        let got = cache.get_by_id(id1).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "NGC 224");
    }

    #[tokio::test]
    async fn user_override_is_sticky_against_resolved() {
        let cache = MemoryCache::new();
        let (id, _) = cache.upsert(&m31(TargetSource::UserOverride), &ns()).await.unwrap();

        let mut later = m31(TargetSource::Resolved);
        later.primary_designation = "WRONG".to_owned();
        let (id2, outcome) = cache.upsert(&later, &ns()).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::SkippedUserOverride);
        assert_eq!(id, id2);

        let got = cache.get_by_id(id).await.unwrap().unwrap();
        assert_eq!(got.primary_designation, "M 31");
        assert_eq!(got.source, TargetSource::UserOverride);
    }

    #[tokio::test]
    async fn user_override_overwrites_resolved() {
        let cache = MemoryCache::new();
        cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

        let mut override_identity = m31(TargetSource::UserOverride);
        override_identity.primary_designation = "Andromeda".to_owned();
        let (_, outcome) = cache.upsert(&override_identity, &ns()).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Updated);

        let got = cache.get_by_simbad_oid(1_575_544).await.unwrap().unwrap();
        assert_eq!(got.source, TargetSource::UserOverride);
        assert_eq!(got.primary_designation, "Andromeda");
    }

    #[tokio::test]
    async fn resolved_refreshes_existing_resolved_row() {
        let cache = MemoryCache::new();
        cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

        let mut refreshed = m31(TargetSource::Resolved);
        refreshed.dec_deg = 41.0;
        let (_, outcome) = cache.upsert(&refreshed, &ns()).await.unwrap();
        assert_eq!(outcome, UpsertOutcome::Updated);

        let got = cache.get_by_simbad_oid(1_575_544).await.unwrap().unwrap();
        assert!((got.dec_deg - 41.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn null_oid_dedups_by_derived_id() {
        let cache = MemoryCache::new();
        let mut seed = m31(TargetSource::Seed);
        seed.simbad_oid = None;
        let (id1, o1) = cache.upsert(&seed, &ns()).await.unwrap();
        assert_eq!(o1, UpsertOutcome::Inserted);

        let (id2, o2) = cache.upsert(&seed, &ns()).await.unwrap();
        assert_eq!(id1, id2);
        assert_eq!(o2, UpsertOutcome::Updated);
        assert_eq!(cache.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let cache = MemoryCache::new();
        assert!(cache.get_by_simbad_oid(999).await.unwrap().is_none());
        assert!(cache.get_by_normalized("nothing").await.unwrap().is_none());
        assert!(cache.get_by_id(Uuid::new_v4()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn aliases_rewritten_on_update() {
        let cache = MemoryCache::new();
        cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

        let mut fewer = m31(TargetSource::Resolved);
        fewer.aliases = vec![ResolvedAlias::new("M 31", AliasKind::Designation)];
        let (id, _) = cache.upsert(&fewer, &ns()).await.unwrap();
        let got = cache.get_by_id(id).await.unwrap().unwrap();
        assert_eq!(got.aliases.len(), 1);
    }

    // ── search ranking ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn search_blank_query_is_empty() {
        let cache = MemoryCache::new();
        seeded(&cache).await;
        assert!(cache.search("   ", 20).await.unwrap().is_empty());
        assert!(cache.search("M31", 0).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_exact_match_ranks_first() {
        let cache = MemoryCache::new();
        seeded(&cache).await;
        let hits = cache.search("NGC 5457", 20).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].rank, RANK_EXACT);
        assert_eq!(hits[0].target.primary_designation, "M 101");
        assert_eq!(hits[0].matched_alias, "NGC 5457");
    }

    #[tokio::test]
    async fn search_prefix_matches_both_ngc() {
        let cache = MemoryCache::new();
        seeded(&cache).await;
        let hits = cache.search("NGC", 20).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.rank == RANK_PREFIX));
    }

    #[tokio::test]
    async fn search_substring_matches_common_name() {
        let cache = MemoryCache::new();
        seeded(&cache).await;
        let hits = cache.search("galaxy", 20).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.rank == RANK_SUBSTRING));
    }

    #[tokio::test]
    async fn search_dedupes_one_hit_per_target() {
        let cache = MemoryCache::new();
        let mut t = m31(TargetSource::Resolved);
        t.aliases = vec![
            ResolvedAlias::new("Andromeda", AliasKind::CommonName),
            ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName),
        ];
        cache.upsert(&t, &ns()).await.unwrap();

        let hits = cache.search("andromeda", 20).await.unwrap();
        assert_eq!(hits.len(), 1, "one canonical target despite two matching aliases");
        assert_eq!(hits[0].rank, RANK_EXACT);
        assert_eq!(hits[0].matched_alias, "Andromeda");
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let cache = MemoryCache::new();
        seeded(&cache).await;
        let hits = cache.search("galaxy", 1).await.unwrap();
        assert_eq!(hits.len(), 1);
    }

    // ── user aliases ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn add_user_alias_is_idempotent() {
        let cache = MemoryCache::new();
        let (id, _) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

        assert!(cache.add_user_alias(id, "M31 favorite").await.unwrap());
        assert!(!cache.add_user_alias(id, "M31 favorite").await.unwrap(), "idempotent re-add");

        let got = cache.get_by_id(id).await.unwrap().unwrap();
        assert_eq!(got.aliases.len(), 4);
        assert!(got.aliases.iter().any(|a| a.kind == AliasKind::User && a.alias == "M31 favorite"));
    }

    #[tokio::test]
    async fn remove_user_alias_only_removes_user_kind() {
        let cache = MemoryCache::new();
        let (id, _) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();
        cache.add_user_alias(id, "M31 favorite").await.unwrap();

        // Find the alias_id assigned to the user alias by scanning the internal
        // table indirectly: the only way the public API exposes it is via the
        // insertion call succeeding, so re-derive it through the aliases map.
        let alias_id = cache
            .0
            .aliases
            .iter()
            .find(|e| e.value().alias.alias == "M31 favorite")
            .map(|e| e.key().clone())
            .expect("user alias row must exist");

        assert!(cache.remove_user_alias(&alias_id).await.unwrap());
        let got = cache.get_by_id(id).await.unwrap().unwrap();
        assert_eq!(got.aliases.len(), 3);
        assert!(!got.aliases.iter().any(|a| a.alias == "M31 favorite"));

        // A designation alias's id must not be removable via remove_user_alias.
        let designation_alias_id = cache
            .0
            .aliases
            .iter()
            .find(|e| e.value().alias.alias == "M 31")
            .map(|e| e.key().clone())
            .expect("designation alias row must exist");
        assert!(!cache.remove_user_alias(&designation_alias_id).await.unwrap());
        assert_eq!(cache.get_by_id(id).await.unwrap().unwrap().aliases.len(), 3);
    }

    #[tokio::test]
    async fn remove_user_alias_missing_id_returns_false() {
        let cache = MemoryCache::new();
        assert!(!cache.remove_user_alias("no-such-id").await.unwrap());
    }

    // ── list ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_orders_by_primary_designation() {
        let cache = MemoryCache::new();
        seeded(&cache).await;
        let rows = cache.list().await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].primary_designation, "M 101");
        assert_eq!(rows[1].primary_designation, "M 31");
    }
}
