# Contract: Queue (crate `simbad-resolver-cache`)

Pluggable pending-work store for the async batch resolver. Impls: in-memory (`-cache-memory`) and durable cache-backed (`-cache-sqlite`, `pending_resolution` table). Object-safe via `async-trait`.

```rust
#[async_trait::async_trait]
pub trait Queue: Send + Sync {
    /// Enqueue (idempotent by `id`); no-op if already present.
    async fn enqueue(&self, id: &str, query: &str) -> Result<(), QueueError>;

    /// Claim up to `n` pending items for processing (nearest to FIFO).
    async fn claim_pending(&self, n: usize) -> Result<Vec<PendingItem>, QueueError>;

    /// Content hit: mark resolved, bind target, (attempts unchanged).
    async fn mark_resolved(&self, id: &str, target_id: Uuid) -> Result<(), QueueError>;

    /// Content miss: mark unresolved, attempts += 1.
    async fn mark_unresolved(&self, id: &str) -> Result<(), QueueError>;

    /// Transient failure: leave pending, attempts unchanged (release the claim).
    async fn release(&self, id: &str) -> Result<(), QueueError>;

    async fn get(&self, id: &str) -> Result<Option<PendingItem>, QueueError>;
    async fn pending_count(&self) -> Result<usize, QueueError>;
}
```

- The durable impl persists items in `pending_resolution` so pending work survives a restart.
- Transient (`Network`/`Timeout`/`Disabled`) → `release` (retry later); content miss (`NotFound`/`Ambiguous`/`Parse`) → `mark_unresolved` (FR-011).
- The batch resolver (facade) drives this trait; see [facade.md](./facade.md).
