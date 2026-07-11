//! In-memory (dashmap) Cache and Queue implementations for simbad-resolver.
//!
//! [`MemoryCache`] and [`MemoryQueue`] implement the `simbad-resolver-cache`
//! traits entirely in-process (no persistence) — useful for tests, embedders
//! without a durability requirement, and as the reference behavior the
//! `-cache-sqlite` backend is verified against (SC-006). Part of the
//! `simbad-resolver` workspace; implemented per
//! `specs/001-simbad-target-resolution/`.
#![forbid(unsafe_code)]

mod cache;
mod queue;

pub use cache::MemoryCache;
pub use queue::MemoryQueue;
