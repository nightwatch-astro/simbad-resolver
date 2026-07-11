//! Caller-owned resolver configuration.
//!
//! Settings are supplied by the caller at construction; the library persists
//! none of them.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_enables_online_and_derives_a_stable_namespace() {
        let a = ResolverConfig::new("example.targets");
        let b = ResolverConfig::new("example.targets");
        let c = ResolverConfig::new("other.targets");
        assert!(a.online_enabled);
        assert_eq!(a.namespace, b.namespace, "same seed derives the same namespace");
        assert_ne!(a.namespace, c.namespace, "a different seed derives a different namespace");
    }

    #[test]
    fn with_online_toggles_the_flag() {
        assert!(!ResolverConfig::new("x").with_online(false).online_enabled);
        assert!(ResolverConfig::new("x").with_online(false).with_online(true).online_enabled);
    }

    #[test]
    fn with_namespace_overrides_the_derived_namespace() {
        let explicit = Uuid::from_u128(0x1234_5678_90ab_cdef_1234_5678_90ab_cdef);
        assert_eq!(ResolverConfig::new("x").with_namespace(explicit).namespace, explicit);
    }
}
