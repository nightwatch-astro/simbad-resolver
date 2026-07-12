//! Live SIMBAD integration tests — hit the real CDS TAP service.
//!
//! These are `#[ignore]`-gated so ordinary `cargo test` stays hermetic and
//! fast. Run them explicitly with `cargo test --test live_simbad -- --ignored`,
//! or let the scheduled `live-simbad` CI workflow run them. Assertions are
//! deliberately loose (presence, object type, rough magnitude range) so a
//! SIMBAD catalog/photometry update doesn't turn them red — their job is to
//! catch *schema* drift (e.g. the `allfluxes.V` join breaking), not to pin
//! exact catalog values.

use simbad_resolver::{ObjectType, PositionResolver, Resolver, TapResolver};

fn resolver() -> TapResolver {
    TapResolver::with_defaults().expect("build TAP client")
}

#[tokio::test]
#[ignore = "hits live SIMBAD; run with --ignored or via the live-simbad workflow"]
async fn resolve_m31_enriches_v_magnitude() {
    let id = resolver().resolve("M 31").await.expect("M 31 resolves");
    assert_eq!(id.primary_designation, "M 31");
    assert_eq!(id.object_type, ObjectType::Galaxy);
    assert!(id.simbad_oid.is_some(), "resolved online → has a SIMBAD oid");
    let v = id.v_mag.expect("M 31 has a V magnitude in allfluxes");
    assert!((2.0..5.0).contains(&v), "M 31 V ~3.4, got {v}");
    assert!(id.aliases.iter().any(|a| a.alias == "NGC 224"), "alias set enriched");
}

#[tokio::test]
#[ignore = "hits live SIMBAD; run with --ignored or via the live-simbad workflow"]
async fn resolve_bright_star_has_low_v_magnitude() {
    // Vega (α Lyr) is a canonical bright-star photometry check: V ≈ 0.03.
    let id = resolver().resolve("Vega").await.expect("Vega resolves");
    let v = id.v_mag.expect("Vega has a V magnitude");
    assert!((-1.0..1.5).contains(&v), "Vega V ~0.03, got {v}");
}

#[tokio::test]
#[ignore = "hits live SIMBAD; run with --ignored or via the live-simbad workflow"]
async fn object_without_v_photometry_still_resolves() {
    // A `LEFT OUTER JOIN allfluxes` miss must not drop the object — resolution
    // still succeeds and `v_mag` is simply `None`. Don't hard-assert `None`
    // (SIMBAD may add photometry later); only that the object resolves.
    let id = resolver().resolve("NGC 206").await.expect("NGC 206 resolves");
    assert_eq!(id.primary_designation, "NGC 206");
}

#[tokio::test]
#[ignore = "hits live SIMBAD; run with --ignored or via the live-simbad workflow"]
async fn cone_search_returns_matches_nearest_first() {
    // A small circle around M 31's ICRS position should include M 31 itself.
    let matches =
        resolver().resolve_position(10.684_708, 41.268_75, 0.05, 5).await.expect("cone search");
    assert!(!matches.is_empty(), "expected at least one match near M 31");
    for w in matches.windows(2) {
        assert!(w[0].separation_deg <= w[1].separation_deg, "nearest-first ordering");
    }
}
