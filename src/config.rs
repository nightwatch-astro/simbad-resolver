//! Configuration types: [`SimbadConfig`] (network-backed resolvers) and
//! [`ResolverConfig`] (the cache-first facade).

use std::time::Duration;

use uuid::Uuid;

/// Default SIMBAD TAP sync endpoint (CDS).
pub const DEFAULT_TAP_ENDPOINT: &str = "https://simbad.cds.unistra.fr/simbad/sim-tap/sync";

/// Polite identifying `User-Agent` (CDS norm): includes this crate's version
/// and a contact URL.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "simbad-resolver/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/nightwatch-astro/simbad-resolver)"
);

/// Configuration for a network-backed [`crate::Resolver`] (e.g. the SIMBAD TAP
/// or Sesame resolver).
#[derive(Clone, Debug)]
pub struct SimbadConfig {
    /// Backend endpoint URL (validated by the resolver via `url`; https, or
    /// http loopback for local testing).
    pub endpoint: String,
    /// Per-request timeout; on expiry the resolver returns
    /// [`crate::ResolveError::Timeout`] so callers degrade to seed+cache.
    pub timeout: Duration,
    /// Identifying `User-Agent` header value.
    pub user_agent: String,
}

impl Default for SimbadConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_TAP_ENDPOINT.to_owned(),
            timeout: Duration::from_secs(10),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

impl SimbadConfig {
    /// Build a config from caller-persisted settings, clamping `timeout_secs`
    /// to a minimum of 1 second (a 0s timeout would fail every request).
    #[must_use]
    pub fn from_settings(endpoint: impl Into<String>, timeout_secs: u64) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout: Duration::from_secs(timeout_secs.max(1)),
            user_agent: DEFAULT_USER_AGENT.to_owned(),
        }
    }
}

/// Configuration for the [`crate::SimbadResolver`] facade.
///
/// Settings are supplied by the caller at construction; the library persists
/// none of them.
///
/// ```
/// use simbad_resolver::ResolverConfig;
///
/// // Namespaced by a fixed, application-specific seed so derived target ids
/// // stay stable across runs. See `SimbadResolver::new` for its full role.
/// let config = ResolverConfig::new("my-app.targets").with_fuzzy(0.6);
/// assert!(config.online_enabled);
/// assert_eq!(config.fuzzy_min_score, Some(0.6));
/// ```
///
/// [`SimbadResolver::new`]: crate::SimbadResolver::new
#[derive(Clone, Debug)]
pub struct ResolverConfig {
    /// Whether online resolution is attempted on a cache miss. When `false`,
    /// only the cache is consulted (offline mode).
    pub online_enabled: bool,
    /// The id namespace used to derive stable target ids from designations.
    /// Supply your own seed via [`ResolverConfig::new`] for id continuity.
    pub namespace: Uuid,
    /// Minimum token-set similarity (`0.0..=1.0`) for a fuzzy typeahead hit in
    /// [`crate::SimbadResolver::search`]. `None` (the default) disables fuzzy
    /// matching; exact/prefix/substring ranking is unaffected either way.
    pub fuzzy_min_score: Option<f32>,
}

impl ResolverConfig {
    /// Build a config from an id-namespace seed (e.g. your app's reverse-DNS
    /// name). Online resolution defaults to enabled.
    #[must_use]
    pub fn new(namespace_seed: &str) -> Self {
        Self {
            online_enabled: true,
            namespace: crate::identity::namespace(namespace_seed),
            fuzzy_min_score: None,
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

    /// Enable fuzzy typeahead in [`crate::SimbadResolver::search`]: after
    /// exact/prefix/substring matches, remaining result slots are filled with
    /// hits whose token-set similarity to the query is at least `min_score`
    /// (clamped to `0.0..=1.0`), in a [`crate::RANK_FUZZY`] tier. Off by default.
    /// Does not affect [`crate::SimbadResolver::resolve`], which stays exact.
    #[must_use]
    pub fn with_fuzzy(mut self, min_score: f32) -> Self {
        self.fuzzy_min_score = Some(min_score.clamp(0.0, 1.0));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_cds_tap_endpoint() {
        let c = SimbadConfig::default();
        assert_eq!(c.endpoint, DEFAULT_TAP_ENDPOINT);
        assert_eq!(c.timeout, Duration::from_secs(10));
        assert!(c.user_agent.starts_with("simbad-resolver/"));
    }

    #[test]
    fn from_settings_clamps_zero_timeout_to_one_second() {
        let c = SimbadConfig::from_settings("https://example/tap", 0);
        assert_eq!(c.timeout, Duration::from_secs(1));
        assert_eq!(c.endpoint, "https://example/tap");
    }

    #[test]
    fn from_settings_preserves_larger_timeout() {
        let c = SimbadConfig::from_settings("https://example/tap", 30);
        assert_eq!(c.timeout, Duration::from_secs(30));
    }

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

    #[test]
    fn fuzzy_is_disabled_by_default() {
        assert_eq!(ResolverConfig::new("x").fuzzy_min_score, None);
    }

    #[test]
    fn with_fuzzy_sets_and_clamps_the_threshold() {
        assert_eq!(ResolverConfig::new("x").with_fuzzy(0.5).fuzzy_min_score, Some(0.5));
        assert_eq!(ResolverConfig::new("x").with_fuzzy(2.0).fuzzy_min_score, Some(1.0));
        assert_eq!(ResolverConfig::new("x").with_fuzzy(-1.0).fuzzy_min_score, Some(0.0));
    }
}
