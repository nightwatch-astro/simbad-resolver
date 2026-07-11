//! Live SIMBAD TAP smoke tests.
//!
//! Ignored by default since they reach the real network endpoint; run with
//! `cargo test -p simbad-resolver-tap -- --ignored`.

use simbad_resolver_core::{ObjectType, PositionResolver, Resolver};
use simbad_resolver_tap::SimbadTapResolver;

#[tokio::test]
#[ignore = "hits the live SIMBAD TAP endpoint"]
async fn resolves_m31() {
    let resolver = SimbadTapResolver::with_defaults().expect("client builds");
    let identity = resolver.resolve("M 31").await.expect("M 31 resolves");
    assert_eq!(identity.object_type, ObjectType::Galaxy);
    assert!((identity.ra_deg - 10.684_7).abs() < 0.01);
    assert!((identity.dec_deg - 41.269).abs() < 0.01);
    assert!(identity.aliases.iter().any(|a| a.alias == "NGC 224"));
}

#[tokio::test]
#[ignore = "hits the live SIMBAD TAP endpoint"]
async fn resolves_ngc_7293() {
    let resolver = SimbadTapResolver::with_defaults().expect("client builds");
    let identity = resolver.resolve("NGC 7293").await.expect("NGC 7293 resolves");
    assert_eq!(identity.object_type, ObjectType::PlanetaryNebula);
}

#[tokio::test]
#[ignore = "hits the live SIMBAD TAP endpoint"]
async fn cone_search_near_m31_finds_it() {
    let resolver = SimbadTapResolver::with_defaults().expect("client builds");
    let matches = resolver
        .resolve_position(10.684_708_3, 41.268_75, 0.2, 5)
        .await
        .expect("cone search succeeds");
    assert!(!matches.is_empty());
    let separations: Vec<f64> = matches.iter().map(|m| m.separation_deg).collect();
    let mut sorted = separations.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(separations, sorted, "results must be nearest-first");
    assert!(matches.iter().any(|m| m.identity.primary_designation == "M 31"));
}
