//! [`MemoryQueue`]: an in-memory `Queue` impl backed by `dashmap`.
//!
//! A "claim" in this backend is just a filtered read (no lease/visibility
//! timeout) — safe because the in-process queue has a single consumer and no
//! restart-durability requirement (unlike `-cache-sqlite`'s
//! `pending_resolution` table).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use simbad_resolver_cache::{PendingItem, PendingState, Queue, QueueError};
use uuid::Uuid;

/// One queued item's mutable state, plus an insertion sequence number used to
/// approximate FIFO ordering in [`Queue::claim_pending`].
#[derive(Clone)]
struct PendingRow {
    query: String,
    state: PendingState,
    attempts: i64,
    target_id: Option<Uuid>,
    seq: u64,
}

impl PendingRow {
    fn into_item(self, id: String) -> PendingItem {
        PendingItem {
            id,
            query: self.query,
            state: self.state,
            attempts: self.attempts,
            target_id: self.target_id,
        }
    }
}

#[derive(Default)]
struct Inner {
    items: DashMap<String, PendingRow>,
    next_seq: AtomicU64,
}

/// In-memory, `dashmap`-backed [`Queue`] implementation.
///
/// Cheaply `Clone`-able (an `Arc` handle over the shared item map); every
/// clone observes the same underlying store.
#[derive(Clone, Default)]
pub struct MemoryQueue(Arc<Inner>);

impl MemoryQueue {
    /// Construct an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Queue for MemoryQueue {
    async fn enqueue(&self, id: &str, query: &str) -> Result<(), QueueError> {
        if self.0.items.contains_key(id) {
            return Ok(()); // idempotent no-op
        }
        let seq = self.0.next_seq.fetch_add(1, Ordering::Relaxed);
        self.0.items.insert(
            id.to_owned(),
            PendingRow {
                query: query.to_owned(),
                state: PendingState::Pending,
                attempts: 0,
                target_id: None,
                seq,
            },
        );
        Ok(())
    }

    async fn claim_pending(&self, n: usize) -> Result<Vec<PendingItem>, QueueError> {
        let mut pending: Vec<(String, PendingRow)> = self
            .0
            .items
            .iter()
            .filter(|entry| entry.value().state == PendingState::Pending)
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        pending.sort_by_key(|(_, row)| row.seq);
        pending.truncate(n);
        Ok(pending.into_iter().map(|(id, row)| row.into_item(id)).collect())
    }

    async fn mark_resolved(&self, id: &str, target_id: Uuid) -> Result<(), QueueError> {
        if let Some(mut row) = self.0.items.get_mut(id) {
            row.state = PendingState::Resolved;
            row.target_id = Some(target_id);
        }
        Ok(())
    }

    async fn mark_unresolved(&self, id: &str) -> Result<(), QueueError> {
        if let Some(mut row) = self.0.items.get_mut(id) {
            row.state = PendingState::Unresolved;
            row.attempts += 1;
        }
        Ok(())
    }

    async fn release(&self, id: &str) -> Result<(), QueueError> {
        if let Some(mut row) = self.0.items.get_mut(id) {
            row.state = PendingState::Pending;
        }
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<PendingItem>, QueueError> {
        Ok(self.0.items.get(id).map(|row| row.clone().into_item(id.to_owned())))
    }

    async fn pending_count(&self) -> Result<usize, QueueError> {
        Ok(self.0.items.iter().filter(|entry| entry.value().state == PendingState::Pending).count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_is_idempotent_by_id() {
        let queue = MemoryQueue::new();
        queue.enqueue("a", "M 31").await.unwrap();
        queue.enqueue("a", "different query").await.unwrap();

        let item = queue.get("a").await.unwrap().unwrap();
        assert_eq!(item.query, "M 31", "second enqueue with the same id must be a no-op");
        assert_eq!(queue.pending_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn claim_pending_returns_oldest_first() {
        let queue = MemoryQueue::new();
        queue.enqueue("a", "one").await.unwrap();
        queue.enqueue("b", "two").await.unwrap();
        queue.enqueue("c", "three").await.unwrap();

        let claimed = queue.claim_pending(2).await.unwrap();
        assert_eq!(claimed.len(), 2);
        assert_eq!(claimed[0].id, "a");
        assert_eq!(claimed[1].id, "b");
    }

    #[tokio::test]
    async fn mark_resolved_sets_state_and_target_leaves_attempts() {
        let queue = MemoryQueue::new();
        queue.enqueue("a", "M 31").await.unwrap();
        let target_id = Uuid::new_v4();
        queue.mark_resolved("a", target_id).await.unwrap();

        let item = queue.get("a").await.unwrap().unwrap();
        assert_eq!(item.state, PendingState::Resolved);
        assert_eq!(item.target_id, Some(target_id));
        assert_eq!(item.attempts, 0);
        assert_eq!(queue.pending_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn mark_unresolved_increments_attempts() {
        let queue = MemoryQueue::new();
        queue.enqueue("a", "bogus").await.unwrap();
        queue.mark_unresolved("a").await.unwrap();
        queue.mark_unresolved("a").await.unwrap();

        let item = queue.get("a").await.unwrap().unwrap();
        assert_eq!(item.state, PendingState::Unresolved);
        assert_eq!(item.attempts, 2);
    }

    #[tokio::test]
    async fn release_returns_item_to_pending_without_changing_attempts() {
        let queue = MemoryQueue::new();
        queue.enqueue("a", "M 31").await.unwrap();
        queue.mark_unresolved("a").await.unwrap(); // attempts = 1, state = unresolved
        queue.release("a").await.unwrap();

        let item = queue.get("a").await.unwrap().unwrap();
        assert_eq!(item.state, PendingState::Pending);
        assert_eq!(item.attempts, 1);
        assert_eq!(queue.pending_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn get_missing_is_none() {
        let queue = MemoryQueue::new();
        assert!(queue.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn mutating_missing_id_is_a_no_op() {
        let queue = MemoryQueue::new();
        queue.mark_resolved("nope", Uuid::new_v4()).await.unwrap();
        queue.mark_unresolved("nope").await.unwrap();
        queue.release("nope").await.unwrap();
        assert!(queue.get("nope").await.unwrap().is_none());
    }
}
