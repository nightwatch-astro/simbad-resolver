//! [`SimbadConfig`]: shared configuration for network-backed resolvers.
//!
//! Lives in the pure core (rather than the `-tap`/`-sesame` crates) so the
//! endpoint/timeout/user-agent shape is a single source of truth across
//! resolver backends.

use std::time::Duration;

/// Default SIMBAD TAP sync endpoint (CDS).
pub const DEFAULT_TAP_ENDPOINT: &str = "https://simbad.cds.unistra.fr/simbad/sim-tap/sync";

/// Polite identifying `User-Agent` (CDS norm): includes this crate's version
/// and a contact URL.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "simbad-resolver/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/srobroek/simbad-resolver)"
);

/// Configuration for a network-backed [`crate::Resolver`] (e.g. the SIMBAD TAP
/// or Sesame resolver).
#[derive(Clone, Debug)]
pub struct SimbadConfig {
    /// Backend endpoint URL (validated by the caller crate via `url`; https,
    /// or http loopback for local testing).
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
}
