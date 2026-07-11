//! Facade integration tests (offline, via FakeResolver + in-memory backends).
//!
//! Covers quickstart scenarios S1 (resolve + cache-first), S3 (sticky override),
//! and S4 (batch drain semantics).

use simbad_resolver::memory::{MemoryCache, MemoryQueue};
use simbad_resolver::{
    BatchResolver, Queue, Resolution, ResolverConfig, SimbadResolver, UnresolvedReason,
};
use simbad_resolver_core::{
    AliasKind, FakeResolver, ObjectType, ResolveError, ResolvedAlias, ResolvedIdentity,
    TargetSource,
};

fn m31() -> ResolvedIdentity {
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
        source: TargetSource::Resolved,
    }
}

fn facade(resolver: FakeResolver) -> SimbadResolver<FakeResolver, MemoryCache> {
    SimbadResolver::new(resolver, MemoryCache::default(), ResolverConfig::new("test.targets"))
}

#[tokio::test]
async fn resolves_and_caches_dedup_by_alias() {
    let f = facade(FakeResolver::new().with_response("M 31", m31()));

    // First resolve: online (one resolver call), returns the canonical identity.
    let got = f.resolve("M31").await.unwrap();
    let Resolution::Resolved(t) = got else { panic!("expected resolved, got {got:?}") };
    assert_eq!(t.primary_designation, "M 31");
    assert_eq!(t.object_type, ObjectType::Galaxy);
    assert_eq!(t.otype_raw, "G");

    // Resolving an alias of the SAME object is a cache hit (no second network call).
    let got2 = f.resolve("NGC 224").await.unwrap();
    let Resolution::Resolved(t2) = got2 else { panic!("expected resolved") };
    assert_eq!(t2.id, t.id, "aliases collapse onto one canonical target");
    assert_eq!(f.resolver().call_count(), 1, "second resolve served from cache");
}

#[tokio::test]
async fn unknown_query_is_unresolved_unknown() {
    let f = facade(FakeResolver::new()); // default: NotFound
    match f.resolve("definitely-not-an-object").await.unwrap() {
        Resolution::Unresolved { reason, .. } => assert_eq!(reason, UnresolvedReason::Unknown),
        Resolution::Resolved(t) => panic!("expected unresolved unknown, got resolved {t:?}"),
    }
}

#[tokio::test]
async fn ambiguous_query_is_unresolved_ambiguous() {
    let f =
        facade(FakeResolver::new().with_error(
            "cluster",
            ResolveError::Ambiguous { query: "cluster".to_owned(), count: 3 },
        ));
    match f.resolve("cluster").await.unwrap() {
        Resolution::Unresolved { reason, .. } => assert_eq!(reason, UnresolvedReason::Ambiguous),
        Resolution::Resolved(t) => panic!("expected ambiguous, got resolved {t:?}"),
    }
}

#[tokio::test]
async fn transient_failure_degrades_to_offline() {
    let f =
        facade(FakeResolver::new().with_default_error(ResolveError::Network("down".to_owned())));
    match f.resolve("M 31").await.unwrap() {
        Resolution::Unresolved { reason, .. } => assert_eq!(reason, UnresolvedReason::Offline),
        Resolution::Resolved(t) => panic!("expected offline, got resolved {t:?}"),
    }
}

#[tokio::test]
async fn online_disabled_never_calls_resolver_but_cache_still_works() {
    // Seed the cache while online, then disable online.
    let resolver = FakeResolver::new().with_response("M 31", m31());
    let cache = MemoryCache::default();
    let online = SimbadResolver::new(resolver, cache.clone(), ResolverConfig::new("test.targets"));
    online.resolve("M 31").await.unwrap();

    let offline_resolver = FakeResolver::new().with_response("M 31", m31());
    let offline = SimbadResolver::new(
        offline_resolver,
        cache,
        ResolverConfig::new("test.targets").with_online(false),
    );
    // Cached object still resolves with no resolver call.
    assert!(matches!(offline.resolve("NGC 224").await.unwrap(), Resolution::Resolved(_)));
    // Unknown object → offline (not a network attempt).
    assert!(matches!(
        offline.resolve("unknown").await.unwrap(),
        Resolution::Unresolved { reason: UnresolvedReason::Offline, .. }
    ));
    assert_eq!(offline.resolver().call_count(), 0, "online disabled → no resolver calls");
}

#[tokio::test]
async fn apply_override_is_sticky_user_override() {
    let f = facade(FakeResolver::new().with_response("M 31", m31()));
    let Resolution::Resolved(t) = f.resolve("M 31").await.unwrap() else { panic!() };
    assert_eq!(t.source, TargetSource::Resolved);

    let overridden = f.apply_override(t.id, "My Andromeda").await.unwrap().unwrap();
    assert_eq!(overridden.source, TargetSource::UserOverride);
    assert!(overridden.aliases.iter().any(|a| a.alias == "My Andromeda"));

    // Unknown target id → None.
    assert!(f.apply_override(uuid::Uuid::new_v4(), "x").await.unwrap().is_none());
}

#[tokio::test]
async fn caldwell_query_translates_and_binds_alias() {
    // C 14 → NGC 869 (Double Cluster). The fake resolves NGC 869; the facade
    // translates C 14 → NGC 869, resolves, and binds "C 14" as an alias.
    let ngc869 = ResolvedIdentity {
        simbad_oid: Some(10_001),
        primary_designation: "NGC 869".to_owned(),
        common_name: Some("Double Cluster".to_owned()),
        object_type: ObjectType::OpenCluster,
        otype_raw: "OpC".to_owned(),
        ra_deg: 34.75,
        dec_deg: 57.13,
        aliases: vec![ResolvedAlias::new("NGC 869", AliasKind::Designation)],
        source: TargetSource::Resolved,
    };
    let f = facade(FakeResolver::new().with_response("NGC 869", ngc869));
    let Resolution::Resolved(t) = f.resolve("C 14").await.unwrap() else { panic!() };
    assert_eq!(t.primary_designation, "NGC 869");
    assert!(t.aliases.iter().any(|a| a.alias == "C 14"), "original Caldwell bound as alias");
    // A subsequent C 14 lookup is now a cache hit.
    assert!(matches!(f.resolve("C 14").await.unwrap(), Resolution::Resolved(_)));
}

#[tokio::test]
async fn batch_drain_resolves_misses_and_retries_transient() {
    let resolver = FakeResolver::new()
        .with_response("M 31", m31())
        .with_error("timeout-one", ResolveError::Timeout(10));
    // default (unregistered) → NotFound (content miss)
    let batch = BatchResolver::new(
        resolver,
        MemoryCache::default(),
        MemoryQueue::default(),
        ResolverConfig::new("test.targets"),
    );

    batch.enqueue("img1", "M 31").await.unwrap();
    batch.enqueue("img2", "unknown-object").await.unwrap();
    batch.enqueue("img3", "NGC 224").await.unwrap(); // alias of M31 → cache hit after img1
    batch.enqueue("img4", "timeout-one").await.unwrap();

    let summary = batch.drain().await.unwrap();
    assert_eq!(summary.resolved, 2, "img1 (M 31) + img3 (NGC 224 via cache) resolved");
    assert_eq!(summary.unresolved, 1, "img2 unknown → unresolved");
    assert_eq!(summary.still_pending, 1, "img4 transient → still pending");

    // Transient item retains its pending state and attempt budget (attempts == 0).
    let q = batch.queue();
    let pending = q.get("img4").await.unwrap().unwrap();
    assert_eq!(pending.state, simbad_resolver::PendingState::Pending);
    assert_eq!(pending.attempts, 0, "transient failure does not consume an attempt");

    let unresolved = q.get("img2").await.unwrap().unwrap();
    assert_eq!(unresolved.state, simbad_resolver::PendingState::Unresolved);
    assert_eq!(unresolved.attempts, 1, "content miss consumes an attempt");
}

/// SC-006: the SAME facade code path works against the durable SQLite backend,
/// unchanged — demonstrating cache-backend substitutability.
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn resolve_flow_is_backend_agnostic_sqlite() {
    let store = simbad_resolver::sqlite::SqliteStore::in_memory().await.unwrap();
    let resolver = FakeResolver::new().with_response("M 31", m31());
    let f = SimbadResolver::new(resolver, store.cache(), ResolverConfig::new("test.targets"));

    let Resolution::Resolved(t) = f.resolve("M31").await.unwrap() else {
        panic!("expected resolved")
    };
    assert_eq!(t.primary_designation, "M 31");
    assert_eq!(t.object_type, ObjectType::Galaxy);

    // Alias dedup + cache-first hold on SQLite exactly as on memory.
    let Resolution::Resolved(t2) = f.resolve("NGC 224").await.unwrap() else { panic!() };
    assert_eq!(t2.id, t.id);
    assert_eq!(f.resolver().call_count(), 1, "second resolve served from the SQLite cache");
}
