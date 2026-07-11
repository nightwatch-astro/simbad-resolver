# Contract: Facade + orchestration (crate `simbad-resolver`)

The main installable crate. Holds a `Resolver`, a `Cache`, an id `namespace`, and config; orchestrates cache-first resolve, sticky override, and the async batch resolver over a `Queue`.

```rust
pub struct SimbadResolver<R: Resolver, C: Cache> {
    resolver: R,
    cache: C,
    namespace: Uuid,          // caller-configurable id namespace (R6)
    online_enabled: bool,     // caller-owned config (R9)
}

pub enum Resolution {
    Resolved(CachedTarget),
    Unresolved { query: String, reason: UnresolvedReason }, // Offline | Unknown | Ambiguous
}

impl<R: Resolver, C: Cache> SimbadResolver<R, C> {
    /// Cache-first resolve: cache hit → return; miss + online → resolve, upsert,
    /// return; transient/offline → Unresolved(Offline); content miss → Unresolved(Unknown/Ambiguous).
    pub async fn resolve(&self, query: &str) -> Result<Resolution, Error>;

    /// Local typeahead (no network) — delegates to `cache.search`.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, Error>;

    /// Bind a query/alias to a chosen target as `user-override` (sticky).
    pub async fn apply_override(&self, target_id: Uuid, alias: &str) -> Result<CachedTarget, Error>;
}

/// Async batch resolver over a pluggable Queue.
pub struct BatchResolver<R, C, Q> { /* resolver, cache, queue, concurrency, namespace */ }
impl<R: Resolver, C: Cache, Q: Queue> BatchResolver<R, C, Q> {
    pub async fn enqueue(&self, id: &str, query: &str) -> Result<(), Error>;
    /// Drain: claim pending, resolve cache-first then online within concurrency,
    /// mark_resolved / mark_unresolved / release per outcome. Returns a summary.
    pub async fn drain(&self) -> Result<DrainSummary, Error>;
}
```

- Caldwell (`C n`) queries are translated via `simbad-resolver-caldwell` before resolving; the original `C n` is bound as an alias (FR-015).
- `resolve` upserts through `cache.upsert(identity, &namespace)`; precedence keeps overrides sticky.
- The facade re-exports `simbad-resolver-core` and, behind feature flags, the concrete backends (`tap`, `sesame`, `cache-memory`, `cache-sqlite`) for a batteries-included import.
- Ecosystem: the returned `CachedTarget`/`SearchHit` (with ra/dec) feed the downstream `target-match` crate; coordinates migrate to `astro_angle::Equatorial` later (R11/R12).
