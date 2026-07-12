//! Pure TSV/wire parsing helpers for a SIMBAD TAP `basic ⋈ allfluxes` row.
//!
//! This crate stays free of a `csv`-crate tokenizer, so [`parse_basic_row`]
//! splits on the tab delimiter directly. SIMBAD's TAP `format=tsv` output
//! never quotes a tab inside a string column, so a plain `split('\t')` is
//! equivalent to the RFC-4180 reader for this fixed 6-column shape
//! (`oid, main_id, ra, dec, otype_txt, V`).

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

/// Parse a `basic ⋈ allfluxes` TSV line into
/// `(oid, main_id, ra, dec, otype, v_mag)`.
///
/// RA/Dec are range-validated (ICRS J2000: `ra ∈ [0, 360)`, `dec ∈ [-90, 90]`)
/// so an out-of-range or non-finite value degrades to `None` (a no-result row)
/// rather than reaching a caller's storage layer with an invalid coordinate.
///
/// The trailing V column is optional: SIMBAD emits an empty field for an object
/// with no V photometry (a `LEFT OUTER JOIN allfluxes` miss), which parses to
/// `v_mag = None`. A non-empty but unparsable/non-finite V also degrades to
/// `None` — magnitude is non-critical and never drops the whole row.
#[must_use]
pub fn parse_basic_row(line: &str) -> Option<(i64, String, f64, f64, String, Option<f64>)> {
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 6 {
        return None;
    }
    let oid: i64 = unquote(fields[0]).parse().ok()?;
    let main_id = unquote(fields[1]);
    let ra: f64 = unquote(fields[2]).parse().ok()?;
    let dec: f64 = unquote(fields[3]).parse().ok()?;
    let otype = unquote(fields[4]);
    let v_mag = parse_optional_mag(fields[5]);

    if !ra.is_finite() || !(0.0..360.0).contains(&ra) {
        return None;
    }
    if !dec.is_finite() || !(-90.0..=90.0).contains(&dec) {
        return None;
    }
    Some((oid, main_id, ra, dec, otype, v_mag))
}

/// Parse an optional magnitude TSV field: empty → `None`; otherwise a finite
/// `f64` → `Some`, or `None` if unparsable/non-finite.
#[must_use]
pub fn parse_optional_mag(field: &str) -> Option<f64> {
    let s = unquote(field);
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
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
        let line = "1575544\t\"M  31\"\t10.6847083\t41.26875\t\"G\"\t3.44";
        let (oid, main_id, ra, dec, otype, v_mag) = parse_basic_row(line).unwrap();
        assert_eq!(oid, 1_575_544);
        assert_eq!(main_id, "M  31");
        assert!((ra - 10.684_708_3).abs() < 1e-6);
        assert!((dec - 41.268_75).abs() < 1e-6);
        assert_eq!(otype, "G");
        assert_eq!(v_mag, Some(3.44));
    }

    #[test]
    fn parse_basic_row_empty_v_is_none() {
        // A LEFT OUTER JOIN miss yields an empty trailing field, not a dropped row.
        let line = "1575544\t\"M  31\"\t10.6847083\t41.26875\t\"G\"\t";
        let (_, _, _, _, _, v_mag) = parse_basic_row(line).unwrap();
        assert_eq!(v_mag, None);
    }

    #[test]
    fn parse_basic_row_unparsable_v_is_none_not_dropped() {
        let line = "1575544\t\"M  31\"\t10.6847083\t41.26875\t\"G\"\t~";
        let row = parse_basic_row(line).unwrap();
        assert_eq!(row.5, None);
    }

    #[test]
    fn parse_basic_row_rejects_short_lines() {
        assert!(parse_basic_row("1\t2\t3").is_none());
        // Five columns (pre-V shape) is now too short.
        assert!(parse_basic_row("1\t\"X\"\t10.0\t41.0\t\"G\"").is_none());
    }

    #[test]
    fn parse_basic_row_rejects_non_numeric_fields() {
        assert!(parse_basic_row("abc\t\"M 31\"\t10.0\t41.0\t\"G\"\t3.4").is_none());
    }

    #[test]
    fn parse_basic_row_rejects_ra_out_of_range() {
        // ra must be in [0, 360); 360.0 and negative values are rejected.
        assert!(parse_basic_row("1\t\"X\"\t360.0\t0.0\t\"G\"\t3.4").is_none());
        assert!(parse_basic_row("1\t\"X\"\t-1.0\t0.0\t\"G\"\t3.4").is_none());
    }

    #[test]
    fn parse_basic_row_rejects_dec_out_of_range() {
        // dec must be in [-90, 90]; 90.0/-90.0 are valid boundaries.
        assert!(parse_basic_row("1\t\"X\"\t10.0\t90.1\t\"G\"\t3.4").is_none());
        assert!(parse_basic_row("1\t\"X\"\t10.0\t-90.1\t\"G\"\t3.4").is_none());
        assert!(parse_basic_row("1\t\"X\"\t10.0\t90.0\t\"G\"\t3.4").is_some());
    }

    #[test]
    fn parse_basic_row_rejects_non_finite_coordinates() {
        assert!(parse_basic_row("1\t\"X\"\tnan\t0.0\t\"G\"\t3.4").is_none());
        assert!(parse_basic_row("1\t\"X\"\t10.0\tinf\t\"G\"\t3.4").is_none());
    }
}
