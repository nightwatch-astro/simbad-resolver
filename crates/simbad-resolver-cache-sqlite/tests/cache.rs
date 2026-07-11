//! Integration tests for [`SqliteCache`] against an in-memory, migrated store.

use simbad_resolver_cache::{Cache, UpsertOutcome};
use simbad_resolver_cache_sqlite::SqliteStore;
use simbad_resolver_core::identity::namespace;
use simbad_resolver_core::normalize::normalize;
use simbad_resolver_core::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};
use uuid::Uuid;

fn ns() -> Uuid {
    namespace("simbad-resolver-cache-sqlite.tests")
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
        aliases: vec![ResolvedAlias::new("NGC 5457", AliasKind::Designation)],
        source: TargetSource::Seed,
    }
}

async fn seeded(cache: &impl Cache) {
    cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();
    cache.upsert(&m101(), &ns()).await.unwrap();
}

// ── upsert / get_by_* ────────────────────────────────────────────────────────

#[tokio::test]
async fn insert_then_read_by_oid() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let (id, outcome) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();
    assert_eq!(outcome, UpsertOutcome::Inserted);

    let got = cache.get_by_simbad_oid(1_575_544).await.unwrap().unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.primary_designation, "M 31");
    assert_eq!(got.object_type, ObjectType::Galaxy);
    assert_eq!(got.otype_raw, "G");
    assert_eq!(got.source, TargetSource::Resolved);
    assert_eq!(got.common_name.as_deref(), Some("Andromeda Galaxy"));
    // "M 31" (primary), "NGC 224" (alias), "Andromeda Galaxy" (common name).
    assert_eq!(got.aliases.len(), 3);
}

#[tokio::test]
async fn read_by_normalized_alias() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

    let got = cache.get_by_normalized(&normalize("NGC 224")).await.unwrap().unwrap();
    assert_eq!(got.primary_designation, "M 31");

    let got2 = cache.get_by_normalized(&normalize("Andromeda Galaxy")).await.unwrap().unwrap();
    assert_eq!(got2.id, got.id);
}

#[tokio::test]
async fn dedup_by_oid_updates_single_row() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let (id1, _) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

    // Re-resolve the SAME oid under a different primary designation; must
    // reuse the existing row, not create a second.
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
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let (id, _) = cache.upsert(&m31(TargetSource::UserOverride), &ns()).await.unwrap();

    // A later SIMBAD resolution must NOT overwrite the override.
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
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

    let mut over = m31(TargetSource::UserOverride);
    over.primary_designation = "Andromeda".to_owned();
    let (_, outcome) = cache.upsert(&over, &ns()).await.unwrap();
    assert_eq!(outcome, UpsertOutcome::Updated);

    let got = cache.get_by_simbad_oid(1_575_544).await.unwrap().unwrap();
    assert_eq!(got.source, TargetSource::UserOverride);
    assert_eq!(got.primary_designation, "Andromeda");
}

#[tokio::test]
async fn resolved_refreshes_existing() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
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
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let mut seed = m31(TargetSource::Seed);
    seed.simbad_oid = None;

    let (id1, o1) = cache.upsert(&seed, &ns()).await.unwrap();
    assert_eq!(o1, UpsertOutcome::Inserted);

    // Same designation, still no oid -> same derived id, updated not inserted.
    let (id2, o2) = cache.upsert(&seed, &ns()).await.unwrap();
    assert_eq!(id1, id2);
    assert_eq!(o2, UpsertOutcome::Updated);
    assert_eq!(cache.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    assert!(cache.get_by_simbad_oid(999).await.unwrap().is_none());
    assert!(cache.get_by_normalized("nothing").await.unwrap().is_none());
    assert!(cache.get_by_id(Uuid::new_v4()).await.unwrap().is_none());
}

#[tokio::test]
async fn aliases_rewritten_on_update() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

    let mut fewer = m31(TargetSource::Resolved);
    fewer.aliases = Vec::new();
    fewer.common_name = None;
    let (id, _) = cache.upsert(&fewer, &ns()).await.unwrap();

    let got = cache.get_by_id(id).await.unwrap().unwrap();
    // Only the primary designation alias survives.
    assert_eq!(got.aliases.len(), 1);
    assert_eq!(got.aliases[0].alias, "M 31");
    assert!(got.common_name.is_none());
}

// ── user aliases ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_user_alias_then_search_and_remove() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let (id, _) = cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

    assert!(cache.add_user_alias(id, "My Fave Galaxy").await.unwrap());
    // Idempotent: adding the same alias again reports "already existed".
    assert!(!cache.add_user_alias(id, "My Fave Galaxy").await.unwrap());

    let got = cache.get_by_id(id).await.unwrap().unwrap();
    let user_alias =
        got.aliases.iter().find(|a| a.kind == AliasKind::User).expect("user alias present");
    assert_eq!(user_alias.alias, "My Fave Galaxy");

    // Only a `kind = 'user'` row is removable; a designation is not.
    let designation_id_removed = cache.remove_user_alias("not-a-real-id").await.unwrap();
    assert!(!designation_id_removed);
}

// ── typeahead search ─────────────────────────────────────────────────────────

#[tokio::test]
async fn search_blank_query_is_empty() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;
    assert!(cache.search("   ", 20).await.unwrap().is_empty());
    assert!(cache.search("M31", 0).await.unwrap().is_empty());
}

#[tokio::test]
async fn search_exact_then_prefix_then_substring_ranking() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;

    let hits = cache.search("NGC 5457", 20).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rank, simbad_resolver_cache::RANK_EXACT);
    assert_eq!(hits[0].target.primary_designation, "M 101");
    assert_eq!(hits[0].matched_alias, "NGC 5457");
}

#[tokio::test]
async fn search_prefix_matches_both_ngc() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;

    let hits = cache.search("NGC", 20).await.unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.rank == simbad_resolver_cache::RANK_PREFIX));
}

#[tokio::test]
async fn search_substring_matches_common_name() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;

    let hits = cache.search("galaxy", 20).await.unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|h| h.rank == simbad_resolver_cache::RANK_SUBSTRING));
}

#[tokio::test]
async fn search_dedupes_one_hit_per_target() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let mut t = m31(TargetSource::Resolved);
    t.common_name = Some("Andromeda".to_owned());
    t.aliases = vec![ResolvedAlias::new("Andromeda Galaxy", AliasKind::CommonName)];
    cache.upsert(&t, &ns()).await.unwrap();

    let hits = cache.search("andromeda", 20).await.unwrap();
    assert_eq!(hits.len(), 1, "one canonical target despite two matching aliases");
    assert_eq!(hits[0].rank, simbad_resolver_cache::RANK_EXACT);
    assert_eq!(hits[0].matched_alias, "Andromeda");
}

#[tokio::test]
async fn search_respects_limit() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;
    let hits = cache.search("galaxy", 1).await.unwrap();
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn search_like_wildcards_are_literal() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;
    // "%" must not act as a wildcard -- no alias literally contains it.
    assert!(cache.search("%", 20).await.unwrap().is_empty());
    assert!(cache.search("_", 20).await.unwrap().is_empty());
}

// ── list ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_carries_aliases_for_resolved_target() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    cache.upsert(&m31(TargetSource::Resolved), &ns()).await.unwrap();

    let rows = cache.list().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].primary_designation, "M 31");
    assert_eq!(rows[0].aliases.len(), 3, "expected 3 aliases, got {:?}", rows[0].aliases);
}

#[tokio::test]
async fn list_orders_by_primary_designation() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await;

    let rows = cache.list().await.unwrap();
    let designations: Vec<&str> = rows.iter().map(|r| r.primary_designation.as_str()).collect();
    let mut sorted = designations.clone();
    sorted.sort_unstable();
    assert_eq!(designations, sorted);
}

#[tokio::test]
async fn list_aliases_do_not_cross_contaminate() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    seeded(&cache).await; // M31 (3 aliases: M31/NGC224/Andromeda Galaxy), M101 (3: M101/NGC5457/Pinwheel Galaxy)

    let rows = cache.list().await.unwrap();
    assert_eq!(rows.len(), 2);

    let m31_row = rows.iter().find(|r| r.primary_designation == "M 31").unwrap();
    let m101_row = rows.iter().find(|r| r.primary_designation == "M 101").unwrap();

    assert_eq!(m31_row.aliases.len(), 3);
    assert_eq!(m101_row.aliases.len(), 3);
    assert!(!m31_row.aliases.iter().any(|a| a.alias == "NGC 5457"));
    assert!(!m101_row.aliases.iter().any(|a| a.alias == "NGC 224"));
}
