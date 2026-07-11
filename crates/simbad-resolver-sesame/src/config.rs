//! Sesame endpoint configuration.
//!
//! Reuses [`SimbadConfig`] (endpoint/timeout/user_agent) rather than
//! introducing a near-duplicate `SesameConfig` type — the shape is
//! identical, only the default endpoint differs.

use simbad_resolver_core::SimbadConfig;

/// Default CDS Sesame `-oxp` endpoint: `S`=SIMBAD, `N`=NED, `V`=VizieR,
/// aggregated in that resolver-precedence order.
///
/// The trailing `?` is intentional and load-bearing: Sesame's CGI query is
/// the percent-encoded object name appended directly after it, not a
/// `key=value` pair (see [`crate::resolver::SimbadSesameResolver`]'s request
/// building).
pub const DEFAULT_SESAME_ENDPOINT: &str = "https://cds.unistra.fr/cgi-bin/nph-sesame/-oxp/SNV?";

/// Build a [`SimbadConfig`] defaulted to the CDS Sesame endpoint (10s
/// timeout, shared `simbad-resolver` user agent) instead of core's default
/// TAP endpoint.
#[must_use]
pub fn default_sesame_config() -> SimbadConfig {
    SimbadConfig { endpoint: DEFAULT_SESAME_ENDPOINT.to_owned(), ..SimbadConfig::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sesame_config_uses_sesame_endpoint() {
        let config = default_sesame_config();
        assert_eq!(config.endpoint, DEFAULT_SESAME_ENDPOINT);
        assert!(config.endpoint.ends_with('?'));
    }
}
