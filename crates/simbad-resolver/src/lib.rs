//! # simbad-resolver
//!
//! Generic, embeddable **SIMBAD astronomical target resolver** for Rust.
//!
//! This is the main installable facade of the `simbad-resolver-*` workspace. It
//! re-exports the resolver ecosystem and provides the higher-level
//! orchestration (cache-first resolve, sticky user-override precedence, and an
//! async batch resolver). The concrete surface is implemented per the spec in
//! `specs/`; this is an initial skeleton.
//!
//! ## Ecosystem
//!
//! `simbad-resolver` is **upstream**: it answers *"what is this name?"* and owns
//! catalog identity (name/identifier → canonical [`ResolvedIdentity`], plus
//! SIMBAD cone-search by position). Two sibling crates compose with it:
//!
//! - [`astro-angle`] — shared coordinate/angle primitives (`Equatorial`, angles,
//!   sexagesimal parse/format). `simbad-resolver` adopts these as its coordinate
//!   type; until then coordinates are plain decimal degrees with a conversion
//!   seam.
//! - [`target-match`] — **downstream**: it answers *"which of these candidates
//!   did this frame capture?"* (pointing + FOV → ranked-by-separation). It
//!   *consumes* `simbad-resolver` output; nothing flows back into the resolver.
//!
//! ```text
//!    name / browse catalog                    frame pointing + FOV
//!            │                                        │
//!            ▼                                        ▼
//!   ┌────────────────────┐   {id, ra, dec}   ┌────────────────────┐
//!   │   simbad-resolver   │ ────────────────▶ │    target-match     │
//!   │ name → identity,    │   candidates      │ rank by angular sep,│
//!   │ SIMBAD TAP+Sesame,  │                   │ FOV/radius geometry │
//!   │ pluggable cache     │                   │ → nearest in-frame  │
//!   └────────────────────┘                   └────────────────────┘
//!            └──────────── both speak astro-angle ──────────────┘
//! ```
//!
//! [`astro-angle`]: https://github.com/srobroek/astro-angle
//! [`target-match`]: https://github.com/srobroek/target-match

#![forbid(unsafe_code)]

// Implemented per specs/. Placeholder to keep the workspace buildable.
