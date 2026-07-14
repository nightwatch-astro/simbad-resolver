// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic `UUIDv5` derivation for a resolved target's stable id.
//!
//! The derivation is caller-namespaced rather than using a single fixed seed:
//! this crate is embedded by many consumers, each of which must own its own
//! namespace to avoid id collisions:
//!
//! ```text
//! ns        = UUIDv5(namespace=dns, name=<caller-supplied seed>)
//! target_id = UUIDv5(namespace=ns, name=<designation>)
//! ```
//!
//! This makes the id stable across machines and catalog updates as long as
//! the seed and the canonical designation do not change.

use uuid::Uuid;

/// Derive a namespace UUID from a caller-supplied seed string.
///
/// Callers should pick a fixed, application-specific seed (e.g.
/// `"my-app.targets"`) and reuse it across runs; the namespace — and
/// therefore every id derived from it — is stable for a given seed.
///
/// This is also what [`crate::ResolverConfig::new`] calls internally to
/// derive [`crate::ResolverConfig::namespace`].
///
/// ```
/// use simbad_resolver::identity::namespace;
///
/// let a = namespace("my-app.targets");
/// let b = namespace("my-app.targets");
/// let c = namespace("other-app.targets");
/// assert_eq!(a, b, "same seed derives the same namespace");
/// assert_ne!(a, c, "different seeds derive different namespaces");
/// ```
#[must_use]
pub fn namespace(seed: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, seed.as_bytes())
}

/// Derive the deterministic `UUIDv5` target id for a canonical `designation`
/// within namespace `ns`.
///
/// The `designation` should be the precedence-winning canonical designation
/// for the resolved object (dedup by `simbad_oid` when available; this is the
/// fallback key otherwise).
///
/// ```
/// use simbad_resolver::identity::{namespace, target_id_from_designation};
///
/// let ns = namespace("my-app.targets");
/// let id1 = target_id_from_designation(&ns, "M 31");
/// let id2 = target_id_from_designation(&ns, "M 31");
/// let id3 = target_id_from_designation(&ns, "M 101");
/// assert_eq!(id1, id2, "same namespace + designation is deterministic");
/// assert_ne!(id1, id3);
/// ```
#[must_use]
pub fn target_id_from_designation(ns: &Uuid, designation: &str) -> Uuid {
    Uuid::new_v5(ns, designation.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_is_deterministic() {
        assert_eq!(namespace("astro-plan.targets"), namespace("astro-plan.targets"));
    }

    #[test]
    fn different_seeds_produce_different_namespaces() {
        assert_ne!(namespace("app-a.targets"), namespace("app-b.targets"));
    }

    #[test]
    fn namespace_is_uuid_v5_sha1() {
        assert_eq!(namespace("astro-plan.targets").get_version(), Some(uuid::Version::Sha1));
    }

    #[test]
    fn target_id_is_deterministic() {
        let ns = namespace("astro-plan.targets");
        let id1 = target_id_from_designation(&ns, "M 31");
        let id2 = target_id_from_designation(&ns, "M 31");
        assert_eq!(id1, id2);
    }

    #[test]
    fn different_designations_produce_different_ids() {
        let ns = namespace("astro-plan.targets");
        let m31 = target_id_from_designation(&ns, "M 31");
        let m101 = target_id_from_designation(&ns, "M 101");
        assert_ne!(m31, m101);
    }

    #[test]
    fn different_namespaces_produce_different_ids_for_same_designation() {
        let a = namespace("app-a.targets");
        let b = namespace("app-b.targets");
        assert_ne!(target_id_from_designation(&a, "M 31"), target_id_from_designation(&b, "M 31"));
    }

    /// Stable/deterministic across the exact `namespace` + `target_id_from_designation`
    /// pairing astro-plan's callers migrate to (data-model.md §Identity derivation).
    #[test]
    fn astro_plan_seed_pipeline_is_stable() {
        let ns = namespace("astro-plan.targets");
        let expected_ns = Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"astro-plan.targets");
        assert_eq!(ns, expected_ns);

        let id = target_id_from_designation(&ns, "M 31");
        let expected_id = Uuid::new_v5(&expected_ns, b"M 31");
        assert_eq!(id, expected_id);
        // Recompute a second time to prove determinism, not just algebraic identity.
        assert_eq!(id, target_id_from_designation(&namespace("astro-plan.targets"), "M 31"));
    }
}
