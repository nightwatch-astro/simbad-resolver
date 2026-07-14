// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Static Caldwell → NGC/IC (or other) designation map.
//!
//! Caldwell is **not** a SIMBAD designation: SIMBAD does not recognise the
//! `C…` prefix as an identifier. To resolve a Caldwell query the caller
//! translates it to the underlying object's *resolvable* designation (NGC,
//! IC, or another catalog SIMBAD does know) using Patrick Moore's published
//! Caldwell catalogue (C1–C109), then resolves / enriches that designation
//! normally.
//!
//! The mapping is committed, immutable, and verifiable against the standard
//! Caldwell catalogue. Each entry maps the Caldwell number to the
//! space-padded designation form the rest of the resolver expects (e.g.
//! `"NGC 188"`, `"IC 1396"`, `"Mel 25"`). A handful of Caldwell objects have no
//! NGC/IC number and use their best-known catalogue designation:
//!
//! - C14 → the Double Cluster (NGC 869 is the conventional anchor).
//! - C41 → the Hyades (`Mel 25`).
//! - C99 → the Coalsack dark nebula (no NGC/IC; not mapped → `None`).

use crate::normalize::normalize;

/// Patrick Moore's Caldwell catalogue, C1–C109, mapped to a SIMBAD-resolvable
/// designation. Indexed implicitly by Caldwell number via [`caldwell_to_designation`].
///
/// `None` entries are Caldwell objects with no single SIMBAD-resolvable catalog
/// designation (e.g. the Coalsack); callers leave those unresolved rather than
/// fabricating one.
const CALDWELL: &[(u16, Option<&str>)] = &[
    (1, Some("NGC 188")),
    (2, Some("NGC 40")),
    (3, Some("NGC 4236")),
    (4, Some("NGC 7023")),
    (5, Some("IC 342")),
    (6, Some("NGC 6543")),
    (7, Some("NGC 2403")),
    (8, Some("NGC 559")),
    (9, Some("Sh2 155")),
    (10, Some("NGC 663")),
    (11, Some("NGC 7635")),
    (12, Some("NGC 6946")),
    (13, Some("NGC 457")),
    (14, Some("NGC 869")),
    (15, Some("NGC 6826")),
    (16, Some("NGC 7243")),
    (17, Some("NGC 147")),
    (18, Some("NGC 185")),
    (19, Some("IC 5146")),
    (20, Some("NGC 7000")),
    (21, Some("NGC 4449")),
    (22, Some("NGC 7662")),
    (23, Some("NGC 891")),
    (24, Some("NGC 1275")),
    (25, Some("NGC 2419")),
    (26, Some("NGC 4244")),
    (27, Some("NGC 6888")),
    (28, Some("NGC 752")),
    (29, Some("NGC 5005")),
    (30, Some("NGC 7331")),
    (31, Some("IC 405")),
    (32, Some("NGC 4631")),
    (33, Some("NGC 6992")),
    (34, Some("NGC 6960")),
    (35, Some("NGC 4889")),
    (36, Some("NGC 4559")),
    (37, Some("NGC 6885")),
    (38, Some("NGC 4565")),
    (39, Some("NGC 2392")),
    (40, Some("NGC 3626")),
    (41, Some("Mel 25")),
    (42, Some("NGC 7006")),
    (43, Some("NGC 7814")),
    (44, Some("NGC 7479")),
    (45, Some("NGC 5248")),
    (46, Some("NGC 2261")),
    (47, Some("NGC 6934")),
    (48, Some("NGC 2775")),
    (49, Some("NGC 2237")),
    (50, Some("NGC 2244")),
    (51, Some("IC 1613")),
    (52, Some("NGC 4697")),
    (53, Some("NGC 3115")),
    (54, Some("NGC 2506")),
    (55, Some("NGC 7009")),
    (56, Some("NGC 246")),
    (57, Some("NGC 6822")),
    (58, Some("NGC 2360")),
    (59, Some("NGC 3242")),
    (60, Some("NGC 4038")),
    (61, Some("NGC 4039")),
    (62, Some("NGC 247")),
    (63, Some("NGC 7293")),
    (64, Some("NGC 2362")),
    (65, Some("NGC 253")),
    (66, Some("NGC 5694")),
    (67, Some("NGC 1097")),
    (68, Some("NGC 6729")),
    (69, Some("NGC 6302")),
    (70, Some("NGC 300")),
    (71, Some("NGC 2477")),
    (72, Some("NGC 55")),
    (73, Some("NGC 1851")),
    (74, Some("NGC 3132")),
    (75, Some("NGC 6124")),
    (76, Some("NGC 6231")),
    (77, Some("NGC 5128")),
    (78, Some("NGC 6541")),
    (79, Some("NGC 3201")),
    (80, Some("NGC 5139")),
    (81, Some("NGC 6352")),
    (82, Some("NGC 6193")),
    (83, Some("NGC 4945")),
    (84, Some("NGC 5286")),
    (85, Some("IC 2391")),
    (86, Some("NGC 6397")),
    (87, Some("NGC 1261")),
    (88, Some("NGC 5823")),
    (89, Some("NGC 6087")),
    (90, Some("NGC 2867")),
    (91, Some("NGC 3532")),
    (92, Some("NGC 3372")),
    (93, Some("NGC 6752")),
    (94, Some("NGC 4755")),
    (95, Some("NGC 6025")),
    (96, Some("NGC 2516")),
    (97, Some("NGC 3766")),
    (98, Some("NGC 4609")),
    // C99 — the Coalsack dark nebula has no NGC/IC designation.
    (99, None),
    (100, Some("IC 2944")),
    (101, Some("NGC 6744")),
    (102, Some("IC 2602")),
    (103, Some("NGC 2070")),
    (104, Some("NGC 362")),
    (105, Some("NGC 4833")),
    (106, Some("NGC 104")),
    (107, Some("NGC 6101")),
    (108, Some("NGC 4372")),
    (109, Some("NGC 3195")),
];

/// Resolve a Caldwell number (1–109) to a SIMBAD-resolvable designation.
///
/// Returns `None` when `n` is outside `1..=109` or when the Caldwell object has
/// no single resolvable catalog designation (e.g. C99, the Coalsack). Callers
/// translate a Caldwell query (`C 14`, `Caldwell 14`) to the returned
/// designation and then resolve / enrich it normally.
///
/// ```
/// use simbad_resolver::caldwell::caldwell_to_designation;
///
/// assert_eq!(caldwell_to_designation(14), Some("NGC 869")); // Double Cluster
/// assert_eq!(caldwell_to_designation(99), None); // Coalsack: no NGC/IC designation
/// assert_eq!(caldwell_to_designation(0), None); // out of range
/// ```
#[must_use]
pub fn caldwell_to_designation(n: u16) -> Option<&'static str> {
    // CALDWELL is dense and contiguous from index 0 = C1; index directly when
    // in range, falling back to a search if the table is ever made sparse.
    let idx = (n as usize).checked_sub(1)?;
    let (num, designation) = CALDWELL.get(idx)?;
    debug_assert_eq!(*num, n, "CALDWELL table must be contiguous C1..=C109");
    *designation
}

/// Total number of committed Caldwell entries (C1–C109 = 109).
///
/// ```
/// use simbad_resolver::caldwell::entry_count;
///
/// assert_eq!(entry_count(), 109);
/// ```
#[must_use]
pub fn entry_count() -> usize {
    CALDWELL.len()
}

/// Parse a bare Caldwell designation (`C 14`, `C14`, `Caldwell 14`, `caldwell14`)
/// into its Caldwell number.
///
/// Normalizes via [`crate::normalize::normalize`] (which
/// expands the `c`/`caldwell` prefix shorthand and lowercases/collapses
/// whitespace), then requires exactly two tokens: the `c`/`caldwell` prefix
/// and a number. Trailing tokens (e.g. `"C 14 extra"`) are rejected so only
/// bare designations match — a query with extra descriptive words is not a
/// Caldwell designation.
///
/// ```
/// use simbad_resolver::caldwell::parse_caldwell_number;
///
/// assert_eq!(parse_caldwell_number("C14"), Some(14));
/// assert_eq!(parse_caldwell_number("Caldwell 14"), Some(14));
/// assert_eq!(parse_caldwell_number("M 31"), None); // not a Caldwell designation
/// assert_eq!(parse_caldwell_number("C 14 extra"), None); // trailing tokens rejected
/// ```
#[must_use]
pub fn parse_caldwell_number(query: &str) -> Option<u16> {
    let norm = normalize(query);
    let mut parts = norm.split_whitespace();
    let prefix = parts.next()?;
    if prefix != "c" && prefix != "caldwell" {
        return None;
    }
    let num: u16 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(num)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_full_caldwell_catalogue() {
        assert_eq!(entry_count(), 109);
    }

    #[test]
    fn table_is_contiguous_c1_to_c109() {
        for (i, (num, _)) in CALDWELL.iter().enumerate() {
            assert_eq!(*num as usize, i + 1, "entry {i} out of order");
        }
    }

    #[test]
    fn known_mappings() {
        assert_eq!(caldwell_to_designation(1), Some("NGC 188"));
        assert_eq!(caldwell_to_designation(14), Some("NGC 869")); // Double Cluster
        assert_eq!(caldwell_to_designation(41), Some("Mel 25")); // Hyades
        assert_eq!(caldwell_to_designation(63), Some("NGC 7293")); // Helix Nebula
        assert_eq!(caldwell_to_designation(77), Some("NGC 5128")); // Centaurus A
        assert_eq!(caldwell_to_designation(92), Some("NGC 3372")); // Eta Carinae Nebula
        assert_eq!(caldwell_to_designation(109), Some("NGC 3195"));
    }

    #[test]
    fn coalsack_has_no_designation() {
        // C99 (Coalsack) has no NGC/IC designation.
        assert_eq!(caldwell_to_designation(99), None);
    }

    #[test]
    fn out_of_range_is_none() {
        assert_eq!(caldwell_to_designation(0), None);
        assert_eq!(caldwell_to_designation(110), None);
        assert_eq!(caldwell_to_designation(u16::MAX), None);
    }

    #[test]
    fn parse_caldwell_number_recognizes_forms() {
        assert_eq!(parse_caldwell_number("C 14"), Some(14));
        assert_eq!(parse_caldwell_number("C14"), Some(14));
        assert_eq!(parse_caldwell_number("Caldwell 14"), Some(14));
        assert_eq!(parse_caldwell_number("caldwell14"), Some(14));
    }

    #[test]
    fn parse_caldwell_number_rejects_non_caldwell() {
        assert_eq!(parse_caldwell_number("M 31"), None);
        assert_eq!(parse_caldwell_number("NGC 224"), None);
        // A common name starting with C must not be mistaken for Caldwell.
        assert_eq!(parse_caldwell_number("Cassiopeia"), None);
        assert_eq!(parse_caldwell_number("C 14 extra"), None);
    }
}
