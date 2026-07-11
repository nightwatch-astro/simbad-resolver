//! [`SqliteQueue`]: the durable, cache-backed `Queue` implementation.

use sqlx::SqlitePool;
use uuid::Uuid;

use simbad_resolver_cache::{PendingItem, PendingState, Queue, QueueError};

/// Raw `pending_resolution` row: id, query, state, attempts, target_id.
type PendingRow = (String, String, String, i64, Option<String>);

// Takes `e` by value (not `&sqlx::Error`) so it can be passed directly as a
// `.map_err(backend_err)` function pointer rather than a closure at each call site.
#[allow(clippy::needless_pass_by_value)]
fn backend_err(e: sqlx::Error) -> QueueError {
    QueueError::Backend(e.to_string())
}

fn row_to_item(row: PendingRow) -> Result<PendingItem, QueueError> {
    let (id, query, state, attempts, target_id) = row;
    let state =
        PendingState::from_wire(&state).ok_or_else(|| QueueError::InvalidState(state.clone()))?;
    let target_id = target_id
        .map(|t| Uuid::parse_str(&t).map_err(|e| QueueError::InvalidUuid(t.clone(), e)))
        .transpose()?;
    Ok(PendingItem { id, query, state, attempts, target_id })
}

/// The durable, SQLite-backed [`Queue`] implementation.
#[derive(Clone, Debug)]
pub struct SqliteQueue {
    pool: SqlitePool,
}

impl SqliteQueue {
    /// Build a queue over an already-migrated pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl Queue for SqliteQueue {
    async fn enqueue(&self, id: &str, query: &str) -> Result<(), QueueError> {
        sqlx::query(
            "INSERT OR IGNORE INTO pending_resolution (id, query, state, attempts)
             VALUES (?, ?, 'pending', 0)",
        )
        .bind(id)
        .bind(query)
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;
        Ok(())
    }

    async fn claim_pending(&self, n: usize) -> Result<Vec<PendingItem>, QueueError> {
        let limit = i64::try_from(n).unwrap_or(i64::MAX);
        // `rowid` (SQLite's implicit insertion-order column) approximates
        // FIFO since the declared `id TEXT PRIMARY KEY` is caller-opaque and
        // not itself ordered.
        let rows: Vec<PendingRow> = sqlx::query_as(
            "SELECT id, query, state, attempts, target_id
             FROM pending_resolution
             WHERE state = 'pending'
             ORDER BY rowid ASC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(backend_err)?;
        rows.into_iter().map(row_to_item).collect()
    }

    async fn mark_resolved(&self, id: &str, target_id: Uuid) -> Result<(), QueueError> {
        sqlx::query("UPDATE pending_resolution SET state = 'resolved', target_id = ? WHERE id = ?")
            .bind(target_id.to_string())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(backend_err)?;
        Ok(())
    }

    async fn mark_unresolved(&self, id: &str) -> Result<(), QueueError> {
        sqlx::query(
            "UPDATE pending_resolution SET state = 'unresolved', attempts = attempts + 1 WHERE id = ?",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;
        Ok(())
    }

    async fn release(&self, id: &str) -> Result<(), QueueError> {
        sqlx::query("UPDATE pending_resolution SET state = 'pending' WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(backend_err)?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<PendingItem>, QueueError> {
        let row: Option<PendingRow> = sqlx::query_as(
            "SELECT id, query, state, attempts, target_id FROM pending_resolution WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend_err)?;
        row.map(row_to_item).transpose()
    }

    async fn pending_count(&self) -> Result<usize, QueueError> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM pending_resolution WHERE state = 'pending'")
                .fetch_one(&self.pool)
                .await
                .map_err(backend_err)?;
        Ok(usize::try_from(count).unwrap_or(0))
    }
}
