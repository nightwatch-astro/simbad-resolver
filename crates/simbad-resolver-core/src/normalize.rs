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
#[must_use]
pub fn tokenize(normalized: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = normalized.split_whitespace().collect();
    tokens.sort_unstable();
    tokens.dedup();
    tokens
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
}
