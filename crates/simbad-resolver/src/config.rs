//! Caller-owned resolver configuration.
//!
//! Settings are supplied by the caller at construction; the library persists
//! none of them (spec clarification 2026-07-11).

use uuid::Uuid;

/// Configuration for the [`crate::SimbadResolver`] facade.
#[derive(Clone, Debug)]
pub struct ResolverConfig {
    /// Whether online resolution is attempted on a cache miss. When `false`,
    /// only the cache is consulted (offline mode).
    pub online_enabled: bool,
    /// The id namespace used to derive stable target ids from designations.
    /// Supply your own seed via [`ResolverConfig::new`] for id continuity.
    pub namespace: Uuid,
}

impl ResolverConfig {
    /// Build a config from an id-namespace seed (e.g. your app's reverse-DNS
    /// name). Online resolution defaults to enabled.
    #[must_use]
    pub fn new(namespace_seed: &str) -> Self {
        Self {
            online_enabled: true,
            namespace: simbad_resolver_core::identity::namespace(namespace_seed),
        }
    }

    /// Set whether online resolution is enabled.
    #[must_use]
    pub fn with_online(mut self, online_enabled: bool) -> Self {
        self.online_enabled = online_enabled;
        self
    }

    /// Use an explicit namespace UUID (rather than deriving it from a seed).
    #[must_use]
    pub fn with_namespace(mut self, namespace: Uuid) -> Self {
        self.namespace = namespace;
        self
    }
}
