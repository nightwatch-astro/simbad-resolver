# Contract: Cache (crate `simbad-resolver-cache`)

The pluggable identity store. Impls: `-cache-memory` (dashmap), `-cache-sqlite` (sqlx). Object-safe via `async-trait`.

```rust
#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CachedTarget>, CacheError>;
    async fn get_by_simbad_oid(&self, oid: i64) -> Result<Option<CachedTarget>, CacheError>;
    /// Exact normalized-alias lookup (normalize the query first).
    async fn get_by_normalized(&self, normalized: &str) -> Result<Option<CachedTarget>, CacheError>;

    /// Ranked typeahead: exact(0) > prefix(1) > substring(2), deduped to one hit
    /// per target, ties broken by shortest matched alias; capped to `limit`.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, CacheError>;

    /// Upsert with dedup (by oid, else derived id) + precedence (R7).
    async fn upsert(&self, identity: &ResolvedIdentity, namespace: &Uuid)
        -> Result<(Uuid, UpsertOutcome), CacheError>;

    /// User-managed aliases + override binding.
    async fn add_user_alias(&self, target_id: Uuid, alias: &str) -> Result<bool, CacheError>;
    async fn remove_user_alias(&self, alias_id: &str) -> Result<bool, CacheError>;

    /// List all (for enumeration / feeding target-match).
    async fn list(&self) -> Result<Vec<CachedTarget>, CacheError>;
}

pub enum UpsertOutcome { Inserted, Updated, SkippedUserOverride }

pub struct SearchHit { pub target: CachedTarget, pub matched_alias: String, pub rank: u8 }

#[derive(thiserror::Error)]
pub enum CacheError { Backend(String), InvalidUuid(..), InvalidEnum(String) }
```

- `upsert` takes the id `namespace` so the derived-id fallback matches the caller's identity scheme (R6).
- A `user-override` identity always wins; a `resolved`/`seed` write into an existing `user-override` row returns `SkippedUserOverride`.
- Aliases are rewritten wholesale on update so re-resolution stays consistent.
- `search` is local-only (no network) and must meet SC-001 (<100 ms) on the SQLite backend via the `target_alias.normalized` index.
- The **same behavior suite** runs against memory + sqlite impls (SC-006).
