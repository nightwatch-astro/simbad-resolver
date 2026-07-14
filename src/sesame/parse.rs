// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tolerant, hand-rolled extraction for CDS Sesame's `-oxp` XML flavour.
//!
//! The workspace has no XML dependency and this crate must not add one
//! (root `Cargo.toml` is out of scope): Sesame's `-oxp` output is a
//! predictable, flat shape (`<Resolver>` blocks containing unnested
//! `<jradeg>`/`<jdedeg>`/`<oname>`/`<otype>`/`<alias>` tags), so a bounded
//! substring scan is adequate and easier to keep correct under a
//! truncated/hostile body than vetting a full parser would be.

use std::fmt::Write as _;

/// One `<Resolver>` block's extracted fields — the first Sesame block that
/// carries valid, range-checked coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SesameHit {
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub oname: Option<String>,
    pub otype: Option<String>,
    pub aliases: Vec<String>,
}

/// Parse a Sesame `-oxp` XML response, returning the fields of the first
/// `<Resolver>` block with valid coordinates (RA ∈ [0, 360), Dec ∈
/// [-90, 90]). Returns `None` when no block has usable coordinates (the
/// caller maps this to `ResolveError::NotFound`).
pub(crate) fn parse_sesame_xml(xml: &str) -> Option<SesameHit> {
    resolver_blocks(xml).into_iter().find_map(parse_block)
}

fn parse_block(block: &str) -> Option<SesameHit> {
    let ra: f64 = extract_first(block, "jradeg")?.parse().ok()?;
    let dec: f64 = extract_first(block, "jdedeg")?.parse().ok()?;
    if !ra.is_finite() || !(0.0..360.0).contains(&ra) {
        return None;
    }
    if !dec.is_finite() || !(-90.0..=90.0).contains(&dec) {
        return None;
    }
    Some(SesameHit {
        ra_deg: ra,
        dec_deg: dec,
        oname: extract_first(block, "oname"),
        otype: extract_first(block, "otype"),
        aliases: extract_all(block, "alias"),
    })
}

/// Split `xml` into `<Resolver>...</Resolver>` blocks, in document order.
///
/// Falls back to treating the whole document as one block when no
/// `<Resolver>` wrapper is found, so a degenerate/minimal fixture (or a
/// future Sesame schema change dropping the wrapper) is still scanned.
fn resolver_blocks(xml: &str) -> Vec<&str> {
    const OPEN: &str = "<Resolver";
    const CLOSE: &str = "</Resolver>";
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while let Some(rel_start) = xml[cursor..].find(OPEN) {
        let start = cursor + rel_start;
        let Some(rel_close) = xml[start..].find(CLOSE) else { break };
        let end = start + rel_close + CLOSE.len();
        blocks.push(&xml[start..end]);
        cursor = end;
    }
    if blocks.is_empty() {
        blocks.push(xml);
    }
    blocks
}

/// The first `<tag>...</tag>` text content in `haystack`, XML-unescaped and
/// trimmed; `None` if the tag is missing, empty, or unclosed.
fn extract_first(haystack: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = haystack.find(&open)? + open.len();
    let rel_end = haystack[start..].find(&close)?;
    let content = haystack[start..start + rel_end].trim();
    if content.is_empty() {
        None
    } else {
        Some(unescape_xml(content))
    }
}

/// Every `<tag>...</tag>` text content in `haystack`, in document order.
fn extract_all(haystack: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel_start) = haystack[cursor..].find(&open) {
        let start = cursor + rel_start + open.len();
        let Some(rel_end) = haystack[start..].find(&close) else { break };
        let end = start + rel_end;
        let content = haystack[start..end].trim();
        if !content.is_empty() {
            out.push(unescape_xml(content));
        }
        cursor = end + close.len();
    }
    out
}

/// Unescape the handful of XML entities Sesame's designation/alias text can
/// contain (object names are not otherwise markup-bearing).
fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
}

/// Percent-encode `name` for embedding directly after the literal `?` in a
/// Sesame endpoint (RFC 3986 unreserved-set encoding).
///
/// Sesame's CGI query is the raw object name, not a `key=value` pair, so
/// `url::Url`'s form-style query builder does not apply here; every
/// non-unreserved byte is encoded so the result is always a valid URL
/// suffix regardless of punctuation in the object name.
pub(crate) fn percent_encode_query(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.trim().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A representative CDS Sesame `-oxp` response for M 31 (SIMBAD block
    /// first, NED second), hand-built to match the documented `-oxp` schema
    /// shape (flat tags inside `<Resolver>` blocks). This is NOT a
    /// byte-for-byte capture of a live response — see the crate-level note
    /// on this being a known ambiguity pending a live-fixture refresh.
    const M31_OXP_FIXTURE: &str = r#"<?xml version='1.0'?>
<Sesame xmlns="http://cdsweb.u-strasbg.fr/~genova/Sesame">
<target name="M31">
<INFO>This is a comment</INFO>
<Resolver name="S=Simbad">
<INFO>from cache</INFO>
<oname>M  31</oname>
<otype>G</otype>
<jpos>00:42:44.330 +41:16:07.50</jpos>
<jradeg>10.68470833</jradeg>
<jdedeg>41.26875000</jdedeg>
<alias>M  31</alias>
<alias>NGC  224</alias>
<alias>Andromeda Galaxy</alias>
</Resolver>
<Resolver name="N=NED">
<oname>MESSIER 031</oname>
<otype>G</otype>
<jradeg>10.684710</jradeg>
<jdedeg>41.269065</jdedeg>
<alias>MESSIER 031</alias>
</Resolver>
</target>
</Sesame>
"#;

    #[test]
    fn parses_ra_dec_primary_and_aliases_from_first_hit_block() {
        let hit = parse_sesame_xml(M31_OXP_FIXTURE).expect("m31 fixture resolves");
        assert!((hit.ra_deg - 10.684_708_33).abs() < 1e-6);
        assert!((hit.dec_deg - 41.268_75).abs() < 1e-6);
        assert_eq!(hit.oname.as_deref(), Some("M  31"));
        assert_eq!(hit.otype.as_deref(), Some("G"));
        assert_eq!(
            hit.aliases,
            vec!["M  31".to_owned(), "NGC  224".to_owned(), "Andromeda Galaxy".to_owned()]
        );
    }

    #[test]
    fn missing_coordinates_returns_none() {
        let xml = r#"<Sesame><target name="Bogus Name Xyz">
<Resolver name="S=Simbad">
<INFO>*** nothing found ***</INFO>
</Resolver>
</target></Sesame>"#;
        assert!(parse_sesame_xml(xml).is_none());
    }

    #[test]
    fn out_of_range_coordinates_are_rejected() {
        let xml =
            r#"<Resolver name="S=Simbad"><jradeg>360.5</jradeg><jdedeg>0.0</jdedeg></Resolver>"#;
        assert!(parse_sesame_xml(xml).is_none());
    }

    #[test]
    fn missing_otype_is_none_not_empty_block() {
        let xml = r#"<Resolver name="S=Simbad"><oname>X</oname><jradeg>1.0</jradeg><jdedeg>1.0</jdedeg></Resolver>"#;
        let hit = parse_sesame_xml(xml).unwrap();
        assert_eq!(hit.otype, None);
        assert_eq!(hit.oname.as_deref(), Some("X"));
    }

    #[test]
    fn percent_encode_query_escapes_space_and_keeps_unreserved_chars() {
        assert_eq!(percent_encode_query("M 31"), "M%2031");
        assert_eq!(percent_encode_query("NGC-224_a.b~c"), "NGC-224_a.b~c");
    }

    #[test]
    fn unescape_xml_handles_common_entities() {
        assert_eq!(
            unescape_xml("A &amp; B &lt;x&gt; &apos;y&apos; &quot;z&quot;"),
            "A & B <x> 'y' \"z\""
        );
    }
}
