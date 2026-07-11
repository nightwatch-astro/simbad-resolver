//! SIMBAD Sesame resolver (broad-coverage name resolve) for simbad-resolver.
//!
//! [`SimbadSesameResolver`] queries CDS Sesame (the `-oxp` XML flavour),
//! which aggregates SIMBAD + NED + VizieR name resolution: broader name
//! coverage than the TAP resolver, but coarser output (no reliable
//! `simbad_oid`, and object-type/alias completeness depends on which
//! backend Sesame answers from first). It optionally enriches a coarse
//! Sesame hit through a caller-supplied [`simbad_resolver_core::Resolver`]
//! trait object (typically the TAP resolver) — this crate has no build
//! dependency on `-tap`.
//!
//! Part of the `simbad-resolver` workspace.
#![forbid(unsafe_code)]

mod config;
mod parse;
mod resolver;

pub use config::{default_sesame_config, DEFAULT_SESAME_ENDPOINT};
pub use resolver::SimbadSesameResolver;
