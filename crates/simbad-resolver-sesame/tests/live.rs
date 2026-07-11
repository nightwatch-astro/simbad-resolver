//! Live network test against the real CDS Sesame service.
//!
//! Ignored by default so offline CI stays green; run explicitly with
//! `cargo test -p simbad-resolver-sesame -- --ignored`. The parser and
//! enrichment-decision unit tests in `src/` already cover the
//! response-shape and fallback contract without a network dependency.

use simbad_resolver_core::Resolver;
use simbad_resolver_sesame::SimbadSesameResolver;

#[tokio::test]
#[ignore = "hits the real CDS Sesame service; run explicitly with --ignored"]
async fn resolves_m31_against_real_sesame() {
    let resolver = SimbadSesameResolver::new();
    let identity = resolver.resolve("M 31").await.expect("Sesame should resolve M 31");
    assert!((identity.ra_deg - 10.68).abs() < 0.5);
    assert!((identity.dec_deg - 41.27).abs() < 0.5);
    assert!(!identity.primary_designation.is_empty());
}
