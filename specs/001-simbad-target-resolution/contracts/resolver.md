# Contract: Resolver (crate `simbad-resolver-core`)

The network-resolution seam. Both TAP and Sesame implement it; `FakeResolver`/`OfflineResolver` are test/degraded impls. Object-safe via `async-trait`.

```rust
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve a designation or common name to one canonical identity.
    async fn resolve(&self, query: &str) -> Result<ResolvedIdentity, ResolveError>;

    /// Resolve a verbatim FITS OBJECT value (defaults to `resolve`).
    async fn resolve_object(&self, object_raw: &str) -> Result<ResolvedIdentity, ResolveError> {
        self.resolve(object_raw).await
    }
}

/// Position resolution (cone search) is a separate capability the TAP resolver
/// provides; not every Resolver supports it.
#[async_trait::async_trait]
pub trait PositionResolver: Send + Sync {
    /// Nearest object(s) within `radius_deg` of (ra_deg, dec_deg), nearest-first.
    async fn resolve_position(
        &self, ra_deg: f64, dec_deg: f64, radius_deg: f64, limit: usize,
    ) -> Result<Vec<PositionMatch>, ResolveError>;
}
```

- `resolve` MUST NOT fabricate: 0 rows → `NotFound`, >1 distinct physical object → `Ambiguous`, transport/timeout → `Network`/`Timeout`, disabled → `Disabled`, malformed → `Parse`.
- `SimbadTapResolver` (crate `-tap`) implements both traits; `SimbadSesameResolver` (crate `-sesame`) implements `Resolver` and takes `Option<Arc<dyn Resolver>>` for optional enrichment.
- `FakeResolver` (feature `test-fixture`) returns canned results keyed by normalized query, with a call counter.

## Config (per resolver crate)

```rust
pub struct SimbadConfig {
    pub endpoint: String,       // validated via `url` (https, or http loopback)
    pub timeout: Duration,      // clamped to >= 1s
    pub user_agent: String,     // caller-supplied; neutral identifying default
}
impl Default for SimbadConfig { /* CDS TAP endpoint, 10s, "simbad-resolver/<ver> (+repo)" */ }
```
