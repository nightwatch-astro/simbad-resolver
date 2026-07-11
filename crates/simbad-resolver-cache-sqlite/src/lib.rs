//! Durable SQLite (sqlx) Cache and cache-backed Queue implementations for simbad-resolver.
//!
//! [`SqliteStore::open`] / [`SqliteStore::in_memory`] run the
//! `migrations/0001_init.sql` schema and hand out [`SqliteCache`] /
//! [`SqliteQueue`] wrappers sharing one `sqlx::SqlitePool`. Both use runtime
//! `sqlx::query`/`query_as` (no compile-time-checked macros, which would need
//! `DATABASE_URL` at build time) per
//! `specs/001-simbad-target-resolution/data-model.md` and the `cache`/`queue`
//! contracts.
#![forbid(unsafe_code)]

mod cache;
mod queue;
mod store;

pub use cache::SqliteCache;
pub use queue::SqliteQueue;
pub use store::SqliteStore;
