//! `Queue` behaviour tests for the redb [`Store`], run against BOTH the
//! in-memory and a temp-file store mode. Folds the coverage of the former
//! dashmap and SQLite queue backends into one suite.

use std::sync::atomic::{AtomicU32, Ordering};

use simbad_resolver::{PendingState, Queue, Store};
use uuid::Uuid;

/// A temp-dir database path unique to this process + call.
fn unique_db_path() -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("simbad-resolver-queue-{}-{n}.redb", std::process::id()))
}

/// For each named `async fn(Store)` test, generate two `#[tokio::test]`s: one
/// over `Store::in_memory()`, one over a temp-file `Store::open(..)`.
macro_rules! both_modes {
    ($($name:ident)*) => {
        $(
            mod $name {
                #[tokio::test]
                async fn in_memory() {
                    super::$name(super::Store::in_memory().expect("in-memory store")).await;
                }
                #[tokio::test]
                async fn file_backed() {
                    let path = super::unique_db_path();
                    super::$name(super::Store::open(&path).expect("file store")).await;
                    let _ = std::fs::remove_file(&path);
                }
            }
        )*
    };
}

both_modes! {
    enqueue_is_idempotent_by_id
    claim_pending_returns_oldest_first
    claim_pending_is_a_peek_not_a_lease
    mark_resolved_sets_state_and_target_leaves_attempts
    mark_unresolved_increments_attempts
    release_returns_item_to_pending_without_changing_attempts
    get_missing_is_none
    mutating_missing_id_is_a_no_op
    pending_count_only_counts_pending_state
    state_persists_across_handles_sharing_the_store
}

async fn enqueue_is_idempotent_by_id(store: Store) {
    let queue = store.queue();
    queue.enqueue("a", "M 31").await.unwrap();
    queue.enqueue("a", "different query").await.unwrap();

    let item = queue.get("a").await.unwrap().unwrap();
    assert_eq!(item.query, "M 31", "second enqueue with the same id must be a no-op");
    assert_eq!(item.state, PendingState::Pending);
    assert_eq!(item.attempts, 0);
    assert_eq!(queue.pending_count().await.unwrap(), 1);
}

async fn claim_pending_returns_oldest_first(store: Store) {
    let queue = store.queue();
    queue.enqueue("a", "one").await.unwrap();
    queue.enqueue("b", "two").await.unwrap();
    queue.enqueue("c", "three").await.unwrap();

    let claimed = queue.claim_pending(2).await.unwrap();
    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].id, "a");
    assert_eq!(claimed[1].id, "b");
}

async fn claim_pending_is_a_peek_not_a_lease(store: Store) {
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

async fn mark_resolved_sets_state_and_target_leaves_attempts(store: Store) {
    let queue = store.queue();
    queue.enqueue("a", "M 42").await.unwrap();
    let target_id = Uuid::new_v4();
    queue.mark_resolved("a", target_id).await.unwrap();

    let item = queue.get("a").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Resolved);
    assert_eq!(item.target_id, Some(target_id));
    assert_eq!(item.attempts, 0);
    assert_eq!(queue.pending_count().await.unwrap(), 0);
}

async fn mark_unresolved_increments_attempts(store: Store) {
    let queue = store.queue();
    queue.enqueue("a", "bogus").await.unwrap();
    queue.mark_unresolved("a").await.unwrap();
    queue.mark_unresolved("a").await.unwrap();

    let item = queue.get("a").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Unresolved);
    assert_eq!(item.attempts, 2);
}

async fn release_returns_item_to_pending_without_changing_attempts(store: Store) {
    let queue = store.queue();
    queue.enqueue("a", "M 31").await.unwrap();
    queue.mark_unresolved("a").await.unwrap(); // attempts = 1, state = unresolved
    queue.release("a").await.unwrap();

    let item = queue.get("a").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Pending);
    assert_eq!(item.attempts, 1, "release must not change attempts");
    assert_eq!(queue.pending_count().await.unwrap(), 1);
}

async fn get_missing_is_none(store: Store) {
    let queue = store.queue();
    assert!(queue.get("nope").await.unwrap().is_none());
}

async fn mutating_missing_id_is_a_no_op(store: Store) {
    let queue = store.queue();
    queue.mark_resolved("nope", Uuid::new_v4()).await.unwrap();
    queue.mark_unresolved("nope").await.unwrap();
    queue.release("nope").await.unwrap();
    assert!(queue.get("nope").await.unwrap().is_none());
}

async fn pending_count_only_counts_pending_state(store: Store) {
    let queue = store.queue();
    queue.enqueue("q1", "a").await.unwrap();
    queue.enqueue("q2", "b").await.unwrap();
    queue.enqueue("q3", "c").await.unwrap();
    queue.mark_unresolved("q2").await.unwrap();

    assert_eq!(queue.pending_count().await.unwrap(), 2);
}

/// Values written through one handle are visible through another sharing the
/// same store — i.e. the store, not any single handle, owns durability.
async fn state_persists_across_handles_sharing_the_store(store: Store) {
    store.queue().enqueue("q1", "m31").await.unwrap();
    let target_id = Uuid::new_v4();

    let queue2 = store.queue();
    assert_eq!(queue2.pending_count().await.unwrap(), 1);
    queue2.mark_resolved("q1", target_id).await.unwrap();

    let queue3 = store.queue();
    let item = queue3.get("q1").await.unwrap().unwrap();
    assert_eq!(item.state, PendingState::Resolved);
    assert_eq!(item.target_id, Some(target_id));
}
