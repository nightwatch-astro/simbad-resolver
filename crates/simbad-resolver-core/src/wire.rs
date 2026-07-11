//! Pure TSV/wire parsing helpers for a SIMBAD TAP `basic` row.
//!
//! Ported from astro-plan's `targeting/resolver/src/simbad.rs`, minus the
//! `csv`-crate tokenizer: this crate stays free of that dependency, so
//! [`parse_basic_row`] splits on the tab delimiter directly. SIMBAD's TAP
//! `format=tsv` output never quotes a tab inside a string column, so a plain
//! `split('\t')` is equivalent to the RFC-4180 reader for this fixed 5-column
//! shape.

/// Strip SIMBAD's surrounding double quotes (TSV string columns are quoted)
/// and outer whitespace.
#[must_use]
pub fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_owned()
}

/// Collapse internal whitespace runs to single spaces and trim
/// (e.g. SIMBAD `"M   31"` → `"M 31"`, `"NGC  224"` → `"NGC 224"`).
#[must_use]
pub fn collapse_spaces(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a `basic`-row TSV line into `(oid, main_id, ra, dec, otype)`.
///
/// RA/Dec are range-validated (ICRS J2000: `ra ∈ [0, 360)`, `dec ∈ [-90, 90]`)
/// so an out-of-range or non-finite value degrades to `None` (a no-result row)
/// rather than reaching a caller's storage layer with an invalid coordinate.
#[must_use]
pub fn parse_basic_row(line: &str) -> Option<(i64, String, f64, f64, String)> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 5 {
        return None;
    }
    let oid: i64 = unquote(fields[0]).parse().ok()?;
    let main_id = unquote(fields[1]);
    let ra: f64 = unquote(fields[2]).parse().ok()?;
    let dec: f64 = unquote(fields[3]).parse().ok()?;
    let otype = unquote(fields[4]);

    if !ra.is_finite() || !(0.0..360.0).contains(&ra) {
        return None;
    }
    if !dec.is_finite() || !(-90.0..=90.0).contains(&dec) {
        return None;
    }
    Some((oid, main_id, ra, dec, otype))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquote_strips_tsv_quotes() {
        assert_eq!(unquote("\"M 31\""), "M 31");
        assert_eq!(unquote("  12345  "), "12345");
    }

    #[test]
    fn collapse_spaces_normalizes_padding() {
        assert_eq!(collapse_spaces("M   31"), "M 31");
        assert_eq!(collapse_spaces("  NGC  224 "), "NGC 224");
    }

    #[test]
    fn parse_basic_row_extracts_columns() {
        let line = "1575544\t\"M  31\"\t10.6847083\t41.26875\t\"G\"";
        let (oid, main_id, ra, dec, otype) = parse_basic_row(line).unwrap();
        assert_eq!(oid, 1_575_544);
        assert_eq!(main_id, "M  31");
        assert!((ra - 10.684_708_3).abs() < 1e-6);
        assert!((dec - 41.268_75).abs() < 1e-6);
        assert_eq!(otype, "G");
    }

    #[test]
    fn parse_basic_row_rejects_short_lines() {
        assert!(parse_basic_row("1\t2\t3").is_none());
    }

    #[test]
    fn parse_basic_row_rejects_non_numeric_fields() {
        assert!(parse_basic_row("abc\t\"M 31\"\t10.0\t41.0\t\"G\"").is_none());
    }

    #[test]
    fn parse_basic_row_rejects_ra_out_of_range() {
        // ra must be in [0, 360); 360.0 and negative values are rejected.
        assert!(parse_basic_row("1\t\"X\"\t360.0\t0.0\t\"G\"").is_none());
        assert!(parse_basic_row("1\t\"X\"\t-1.0\t0.0\t\"G\"").is_none());
    }

    #[test]
    fn parse_basic_row_rejects_dec_out_of_range() {
        // dec must be in [-90, 90]; 90.0/-90.0 are valid boundaries.
        assert!(parse_basic_row("1\t\"X\"\t10.0\t90.1\t\"G\"").is_none());
        assert!(parse_basic_row("1\t\"X\"\t10.0\t-90.1\t\"G\"").is_none());
        assert!(parse_basic_row("1\t\"X\"\t10.0\t90.0\t\"G\"").is_some());
    }

    #[test]
    fn parse_basic_row_rejects_non_finite_coordinates() {
        assert!(parse_basic_row("1\t\"X\"\tnan\t0.0\t\"G\"").is_none());
        assert!(parse_basic_row("1\t\"X\"\t10.0\tinf\t\"G\"").is_none());
    }
}
