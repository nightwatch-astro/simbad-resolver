//! Pool + migration bootstrap for the SQLite backend.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use crate::cache::SqliteCache;
use crate::queue::SqliteQueue;

/// An open, migrated SQLite pool shared by a [`SqliteCache`] and [`SqliteQueue`].
///
/// Cloning a [`sqlx::SqlitePool`] is cheap (it is a handle around a shared
/// connection pool), so [`Self::cache`] / [`Self::queue`] hand out independent
/// wrappers over the same underlying connections.
#[derive(Clone, Debug)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (creating if missing) the SQLite database at `url` and run
    /// pending migrations.
    ///
    /// # Errors
    ///
    /// Returns the underlying `sqlx::Error` if the connection or a migration
    /// fails.
    pub async fn open(url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new().connect(url).await?;
        Self::migrated(pool).await
    }

    /// Open a fresh, migrated in-memory database (for tests).
    ///
    /// The pool is pinned to a single connection: SQLite's `:memory:`
    /// database is private to the connection that created it, so a pool
    /// that opened a second connection would see an empty, unmigrated
    /// database on that connection.
    ///
    /// # Errors
    ///
    /// Returns the underlying `sqlx::Error` if the connection or a migration
    /// fails.
    pub async fn in_memory() -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await?;
        Self::migrated(pool).await
    }

    /// Wrap an already-open pool, running pending migrations on it.
    ///
    /// # Errors
    ///
    /// Returns the underlying `sqlx::Error` if a migration fails.
    async fn migrated(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// Borrow the underlying pool.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// A [`SqliteCache`] over this store's pool.
    #[must_use]
    pub fn cache(&self) -> SqliteCache {
        SqliteCache::new(self.pool.clone())
    }

    /// A [`SqliteQueue`] over this store's pool.
    #[must_use]
    pub fn queue(&self) -> SqliteQueue {
        SqliteQueue::new(self.pool.clone())
    }
}
