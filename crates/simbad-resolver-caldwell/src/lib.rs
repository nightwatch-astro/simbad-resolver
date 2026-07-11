//! Static Caldwell C1-C109 to NGC/IC designation map for simbad-resolver.
//!
//! Part of the `simbad-resolver` workspace. Implemented per
//! `specs/001-simbad-target-resolution/`.
#![forbid(unsafe_code)]

mod caldwell;

pub use caldwell::{caldwell_to_designation, entry_count, parse_caldwell_number};
