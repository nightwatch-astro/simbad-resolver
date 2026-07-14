// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Query normalization pipeline.
//!
//! The pipeline applied by [`normalize`]:
//!
//! 1. NFKC Unicode normalization (compatibility decomposition, canonical
//!    composition) — collapses lookalike characters.
//! 2. Casefold to ASCII lowercase for letter characters.
//! 3. Strip all punctuation characters **except** digits and letters.
//! 4. Collapse runs of internal whitespace to a single space.
//! 5. Trim leading/trailing whitespace.
//! 6. Expand catalog-prefix shorthands:
//!    - `m<digits>` → `m <digits>` (Messier)
//!    - `ngc<digits>` → `ngc <digits>`
//!    - `ic<digits>` → `ic <digits>`
//!    - `sh2<digits>` → `sh2 <digits>` (Sharpless)
//!    - `b<digits>` → `b <digits>` (Barnard)
//!    - `vdb<digits>` → `vdb <digits>`
//!    - `ldn<digits>` → `ldn <digits>`
//!    - `lbn<digits>` → `lbn <digits>`
//!    - `mel<digits>` → `mel <digits>` (Melotte)
//!    - `c<digits>` → `c <digits>` (Caldwell)
//!    - `arp<digits>` → `arp <digits>`

use unicode_normalization::UnicodeNormalization;

/// Normalize a free-form query string for catalog lookup.
///
/// The output is a lowercased, whitespace-collapsed, punctuation-stripped,
/// prefix-expanded string suitable for exact-match hashing and token-set
/// similarity scoring.
///
/// ```
/// use simbad_resolver::normalize::normalize;
///
/// assert_eq!(normalize("M31"), "m 31"); // catalog-prefix expansion
/// assert_eq!(normalize("NGC-5457"), "ngc 5457"); // punctuation stripped
/// assert_eq!(normalize("  M  31  "), "m 31"); // whitespace collapsed + trimmed
/// ```
#[must_use]
pub fn normalize(input: &str) -> String {
    // Stage 1a: NFKC normalization.
    let nfkc: String = input.nfkc().collect();

    // Stage 1b: casefold to ASCII lowercase (astronomy names are ASCII).
    let lower = nfkc.to_lowercase();

    // Stage 1c: strip punctuation (keep letters, digits, whitespace).
    let stripped: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect();

    // Stage 1d: collapse whitespace and trim.
    let collapsed = collapse_spaces(&stripped);

    // Stage 1e: prefix expansion.
    expand_prefixes(&collapsed)
}

/// Collapse runs of whitespace to a single space and trim edges.
fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true; // treat start as space to skip leading spaces
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    // Trim trailing space added above.
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Expand catalog prefix shorthands so `m31` becomes `m 31` etc.
///
/// The pattern is: a known prefix immediately followed by a digit, with no
/// space in between. We insert a single space. If a space already separates
/// the prefix from the number, the result is unchanged.
fn expand_prefixes(s: &str) -> String {
    // Ordered from longest prefix to shortest to avoid ambiguity.
    const PREFIXES: &[&str] = &[
        "abell",
        "sharpless",
        "barnard",
        "openngc",
        "melotte",
        "caldwell",
        "ngc",
        "lbn",
        "ldn",
        "vdb",
        "sh2",
        "mel",
        "arp",
        "ic",
        "m",
        "b",
        "c",
    ];

    for prefix in PREFIXES {
        if let Some(rest) = s.strip_prefix(prefix) {
            // `rest` must start with a digit for this to be a valid prefix expansion.
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                return format!("{prefix} {rest}");
            }
        }
    }
    s.to_owned()
}

/// Tokenize a normalized string into a sorted, deduplicated set of tokens.
///
/// ```
/// use simbad_resolver::normalize::{normalize, tokenize};
///
/// let normalized = normalize("NGC 5457");
/// assert_eq!(tokenize(&normalized), vec!["5457", "ngc"]);
/// ```
#[must_use]
pub fn tokenize(normalized: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = normalized.split_whitespace().collect();
    tokens.sort_unstable();
    tokens.dedup();
    tokens
}

/// Token-set (Jaccard) similarity between two free-form names, in `0.0..=1.0`.
///
/// Both inputs pass through [`normalize`] (so `M31` ≈ `M 31`) and are split into
/// token **sets**; the score is `|A ∩ B| / |A ∪ B|`. This rewards reordered,
/// partial, and extra-word matches — `"galaxy andromeda"` vs `"Andromeda Galaxy"`
/// scores `1.0`, `"Andromeda"` vs `"Andromeda Galaxy"` scores `0.5`. Being
/// token-granular, it does **not** see intra-token typos: `"andromda"` scores
/// `0.0` against `"andromeda"`. Two empty inputs score `0.0`.
///
/// This is what backs the fuzzy tier of [`crate::SimbadResolver::search`] via
/// [`crate::ResolverConfig::with_fuzzy`].
///
/// ```
/// use simbad_resolver::normalize::token_set_similarity;
///
/// let s = token_set_similarity("galaxy andromeda", "Andromeda Galaxy");
/// assert!((s - 1.0).abs() < f32::EPSILON, "reordered tokens still match fully");
///
/// let partial = token_set_similarity("Andromeda", "Andromeda Galaxy");
/// assert!((partial - 0.5).abs() < f32::EPSILON, "1 shared / 2 union tokens");
/// ```
#[must_use]
pub fn token_set_similarity(a: &str, b: &str) -> f32 {
    jaccard_normalized(&normalize(a), &normalize(b))
}

/// Jaccard overlap of the whitespace token sets of two already-[`normalize`]d
/// strings. Callers holding a precomputed normalized form (e.g. a stored alias)
/// use this to avoid renormalizing on every comparison.
#[must_use]
#[allow(clippy::cast_precision_loss)] // token counts are tiny; the ratio is exact in f32
pub(crate) fn jaccard_normalized(a_norm: &str, b_norm: &str) -> f32 {
    use std::collections::HashSet;
    let a: HashSet<&str> = a_norm.split_whitespace().collect();
    let b: HashSet<&str> = b_norm.split_whitespace().collect();
    let intersection = a.intersection(&b).count();
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize ─────────────────────────────────────────────────────────────

    #[test]
    fn normalize_casefolds() {
        assert_eq!(normalize("M31"), "m 31");
    }

    #[test]
    fn normalize_strips_punctuation() {
        assert_eq!(normalize("NGC-5457"), "ngc 5457");
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("m  101"), "m 101");
    }

    #[test]
    fn normalize_expands_m_prefix() {
        assert_eq!(normalize("M31"), "m 31");
        assert_eq!(normalize("m101"), "m 101");
    }

    #[test]
    fn normalize_expands_ngc_prefix() {
        assert_eq!(normalize("NGC224"), "ngc 224");
        assert_eq!(normalize("NGC7000"), "ngc 7000");
    }

    #[test]
    fn normalize_expands_ic_prefix() {
        assert_eq!(normalize("IC1396"), "ic 1396");
    }

    #[test]
    fn normalize_expands_sh2_prefix() {
        assert_eq!(normalize("Sh2-155"), "sh2 155");
    }

    #[test]
    fn normalize_expands_every_catalog_prefix() {
        assert_eq!(normalize("B33"), "b 33");
        assert_eq!(normalize("C14"), "c 14");
        assert_eq!(normalize("Arp273"), "arp 273");
        assert_eq!(normalize("vdB1"), "vdb 1");
        assert_eq!(normalize("LDN1250"), "ldn 1250");
        assert_eq!(normalize("LBN500"), "lbn 500");
        assert_eq!(normalize("Mel15"), "mel 15");
        assert_eq!(normalize("Abell2151"), "abell 2151");
        assert_eq!(normalize("Barnard33"), "barnard 33");
        assert_eq!(normalize("Sharpless155"), "sharpless 155");
        assert_eq!(normalize("Melotte15"), "melotte 15");
        assert_eq!(normalize("Caldwell14"), "caldwell 14");
        assert_eq!(normalize("OpenNGC42"), "openngc 42");
    }

    #[test]
    fn normalize_prefix_without_trailing_digit_is_unchanged() {
        // A bare prefix with no following digit must not gain a space.
        assert_eq!(normalize("M"), "m");
        assert_eq!(normalize("IC"), "ic");
        assert_eq!(normalize("Barnard"), "barnard");
        assert_eq!(normalize("Mel"), "mel");
    }

    #[test]
    fn normalize_with_existing_space_unchanged() {
        assert_eq!(normalize("NGC 224"), "ngc 224");
    }

    #[test]
    fn normalize_plain_name_unchanged() {
        assert_eq!(normalize("Andromeda Galaxy"), "andromeda galaxy");
    }

    #[test]
    fn normalize_trims_whitespace() {
        assert_eq!(normalize("  M31  "), "m 31");
    }

    #[test]
    fn normalize_extra_tokens_preserved() {
        assert_eq!(normalize("M101 LRGB"), "m 101 lrgb");
    }

    #[test]
    fn normalize_empty_string_is_empty() {
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn normalize_generic_word_unchanged() {
        assert_eq!(normalize("Light"), "light");
    }

    // ── tokenize ─────────────────────────────────────────────────────────────

    #[test]
    fn tokenize_splits_and_sorts() {
        let t = tokenize("ngc 5457");
        assert_eq!(t, vec!["5457", "ngc"]);
    }

    #[test]
    fn tokenize_deduplicates() {
        let t = tokenize("m m 31");
        assert_eq!(t, vec!["31", "m"]);
    }

    // ── token_set_similarity ─────────────────────────────────────────────────

    #[test]
    fn similarity_identical_names_score_one() {
        let s = token_set_similarity("Andromeda Galaxy", "andromeda galaxy");
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_is_token_order_insensitive() {
        let s = token_set_similarity("galaxy andromeda", "Andromeda Galaxy");
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_partial_subset_is_ratio_of_shared_tokens() {
        // {andromeda} within {andromeda, galaxy}: 1 shared / 2 union.
        let s = token_set_similarity("Andromeda", "Andromeda Galaxy");
        assert!((s - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_applies_catalog_normalization() {
        // "m31" -> "m 31" {m,31}; "M 31 Galaxy" -> {m,31,galaxy}: 2 shared / 3 union.
        let s = token_set_similarity("m31", "M 31 Galaxy");
        assert!((s - 2.0_f32 / 3.0).abs() < 1e-6, "got {s}");
    }

    #[test]
    fn similarity_disjoint_names_score_zero() {
        assert!(token_set_similarity("Orion Nebula", "Andromeda Galaxy").abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_is_token_granular_not_edit_distance() {
        // A single-character typo shares no whole token -> 0 (documented limitation).
        assert!(token_set_similarity("andromda", "andromeda").abs() < f32::EPSILON);
    }

    #[test]
    fn similarity_empty_inputs_score_zero() {
        assert!(token_set_similarity("", "andromeda").abs() < f32::EPSILON);
        assert!(token_set_similarity("", "").abs() < f32::EPSILON);
    }
}
