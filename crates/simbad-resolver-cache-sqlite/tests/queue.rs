//! Integration tests for [`SqliteQueue`] against an in-memory, migrated store.

use simbad_resolver_cache::{Cache, PendingState, Queue};
use simbad_resolver_cache_sqlite::SqliteStore;
use simbad_resolver_core::identity::namespace;
use simbad_resolver_core::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};

fn some_identity() -> ResolvedIdentity {
    ResolvedIdentity {
        simbad_oid: Some(42),
        primary_designation: "M 42".to_owned(),
        common_name: Some("Orion Nebula".to_owned()),
        object_type: ObjectType::EmissionNebula,
        otype_raw: "HII".to_owned(),
        ra_deg: 83.822_08,
        dec_deg: -5.391_11,
        aliases: vec![ResolvedAlias::new("NGC 1976", AliasKind::Designation)],
        source: TargetSource::Resolved,
    }
}

#[tokio::test]
async fn enqueue_is_idempotent() {
    let store = SqliteStore::in_memory().await.unwrap();
    let queue = store.queue();

    queue.enqueue("q1", "m31").await.unwrap();
    queue.enqueue("q1", "a different query string").await.unwrap();

    // Second enqueue with the same id is a no-op: the original query survives.
    let item = queue.get("q1").await.unwrap().unwrap();
    assert_eq!(item.query, "m31");
    assert_eq!(item.state, PendingState::Pending);
    assert_eq!(item.attempts, 0);
    assert_eq!(queue.pending_count().await.unwrap(), 1);
}

#[tokio::test]
async fn claim_pending_returns_only_pending_up_to_n() {
    let store = SqliteStore::in_memory().await.unwrap();
    let queue = store.queue();
    queue.enqueue("q1", "m31").await.unwrap();
    queue.enqueue("q2", "m101").await.unwrap();
    queue.enqueue("q3", "m42").await.unwrap();

    let claimed = queue.claim_pending(2).await.unwrap();
    assert_eq!(claimed.len(), 2);
    assert!(claimed.iter().all(|i| i.state == PendingState::Pending));

    let all = queue.claim_pending(100).await.unwrap();
    assert_eq!(all.len(), 3, "claim_pending is a peek, not a lease");
}

#[tokio::test]
async fn mark_resolved_sets_state_and_target_attempts_unchanged() {
    let store = SqliteStore::in_memory().await.unwrap();
    let cache = store.cache();
    let queue = store.queue();

    let ns = namespace("simbad-resolver-cache-sqlite.tests");
    let (target_id, _) = cache.upsert(&some_identity(), &ns).await.unwrap();

    queue.enqueue("q1", "m42").await.unwrap();
    queue.mark_resolved("q1", target_id).await.unwrap();

    let item = queue.get("q1").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Resolved);
    assert_eq!(item.target_id, Some(target_id));
    assert_eq!(item.attempts, 0);
    assert_eq!(queue.pending_count().await.unwrap(), 0);
}

#[tokio::test]
async fn mark_unresolved_increments_attempts() {
    let store = SqliteStore::in_memory().await.unwrap();
    let queue = store.queue();
    queue.enqueue("q1", "bogus query").await.unwrap();

    queue.mark_unresolved("q1").await.unwrap();
    let item = queue.get("q1").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Unresolved);
    assert_eq!(item.attempts, 1);

    queue.mark_unresolved("q1").await.unwrap();
    let item = queue.get("q1").await.unwrap().unwrap();
    assert_eq!(item.attempts, 2);
}

#[tokio::test]
async fn release_returns_to_pending_with_attempts_unchanged() {
    let store = SqliteStore::in_memory().await.unwrap();
    let queue = store.queue();
    queue.enqueue("q1", "m31").await.unwrap();

    // Simulate a content miss (bumps attempts), then a transient-failure
    // release afterwards (must NOT touch attempts further).
    queue.mark_unresolved("q1").await.unwrap();
    queue.release("q1").await.unwrap();

    let item = queue.get("q1").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Pending);
    assert_eq!(item.attempts, 1, "release must not change attempts");
    assert_eq!(queue.pending_count().await.unwrap(), 1);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let store = SqliteStore::in_memory().await.unwrap();
    let queue = store.queue();
    assert!(queue.get("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn pending_count_only_counts_pending_state() {
    let store = SqliteStore::in_memory().await.unwrap();
    let queue = store.queue();
    queue.enqueue("q1", "a").await.unwrap();
    queue.enqueue("q2", "b").await.unwrap();
    queue.enqueue("q3", "c").await.unwrap();
    queue.mark_unresolved("q2").await.unwrap();

    assert_eq!(queue.pending_count().await.unwrap(), 2);
}

/// Values written through one handle are visible through another sharing the
/// same pool — i.e. the store, not any single handle, owns durability.
#[tokio::test]
async fn state_persists_across_handles_sharing_the_pool() {
    let store = SqliteStore::in_memory().await.unwrap();
    let ns = namespace("simbad-resolver-cache-sqlite.tests");
    let (target_id, _) = store.cache().upsert(&some_identity(), &ns).await.unwrap();
    store.queue().enqueue("q1", "m31").await.unwrap();

    // A second Queue handle built from the same store sees the same data.
    let queue2 = store.queue();
    assert_eq!(queue2.pending_count().await.unwrap(), 1);
    queue2.mark_resolved("q1", target_id).await.unwrap();

    let queue3 = store.queue();
    let item = queue3.get("q1").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Resolved);
    assert_eq!(item.target_id, Some(target_id));
}
