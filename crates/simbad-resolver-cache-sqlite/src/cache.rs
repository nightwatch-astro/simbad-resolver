//! [`SqliteCache`]: the durable `Cache` implementation.
//!
//! Ported from astro-plan's `targeting/resolver/src/cache.rs` (runtime
//! `sqlx::query`/`query_as`, no compile-time-checked macros — this workspace's
//! `sqlx` feature set omits the offline-macro machinery). Dedup + source
//! precedence follow `specs/001-simbad-target-resolution/data-model.md`.

use std::collections::HashMap;

use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use simbad_resolver_cache::{Cache, CacheError, CachedTarget, SearchHit, UpsertOutcome};
use simbad_resolver_core::identity::target_id_from_designation;
use simbad_resolver_core::normalize::normalize;
use simbad_resolver_core::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};

/// 9-column `canonical_target` row: id, simbad_oid, primary_designation,
/// object_type, otype_raw, ra_deg, dec_deg, source, resolved_at.
type CanonicalTargetRow = (String, Option<i64>, String, String, String, f64, f64, String, String);

// Takes `e` by value (not `&sqlx::Error`) so it can be passed directly as a
// `.map_err(backend_err)` function pointer rather than a closure at each call site.
#[allow(clippy::needless_pass_by_value)]
fn backend_err(e: sqlx::Error) -> CacheError {
    CacheError::Backend(e.to_string())
}

/// The durable, SQLite-backed [`Cache`] implementation.
#[derive(Clone, Debug)]
pub struct SqliteCache {
    pool: SqlitePool,
}

impl SqliteCache {
    /// Build a cache over an already-migrated pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Load the aliases for a target id, ordered by alias.
async fn load_aliases(
    pool: &SqlitePool,
    target_id: &str,
) -> Result<Vec<ResolvedAlias>, CacheError> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT alias, normalized, kind FROM target_alias WHERE target_id = ? ORDER BY alias ASC",
    )
    .bind(target_id)
    .fetch_all(pool)
    .await
    .map_err(backend_err)?;

    Ok(rows
        .into_iter()
        .map(|(alias, normalized, kind)| ResolvedAlias {
            alias,
            normalized,
            kind: AliasKind::from_wire(&kind),
        })
        .collect())
}

/// Assemble a [`CachedTarget`] from a `canonical_target` row plus its aliases.
///
/// The schema has no `common_name` column (data-model.md keeps that column
/// app-specific out of scope); it is reconstructed here as the first alias
/// tagged [`AliasKind::CommonName`], which `upsert` always writes when
/// `identity.common_name` is `Some`.
async fn assemble(pool: &SqlitePool, row: CanonicalTargetRow) -> Result<CachedTarget, CacheError> {
    let (
        id_str,
        simbad_oid,
        primary_designation,
        object_type,
        otype_raw,
        ra_deg,
        dec_deg,
        source,
        resolved_at,
    ) = row;
    let id = Uuid::parse_str(&id_str).map_err(|e| CacheError::InvalidUuid(id_str.clone(), e))?;
    let source =
        TargetSource::from_wire(&source).ok_or_else(|| CacheError::InvalidEnum(source.clone()))?;
    let aliases = load_aliases(pool, &id_str).await?;
    let common_name =
        aliases.iter().find(|a| a.kind == AliasKind::CommonName).map(|a| a.alias.clone());
    Ok(CachedTarget {
        id,
        simbad_oid,
        primary_designation,
        common_name,
        object_type: ObjectType::from_wire(&object_type),
        otype_raw,
        ra_deg,
        dec_deg,
        source,
        resolved_at,
        aliases,
    })
}

const CANONICAL_TARGET_COLUMNS: &str =
    "id, simbad_oid, primary_designation, object_type, otype_raw, ra_deg, dec_deg, source, resolved_at";

async fn get_by_id(pool: &SqlitePool, id: Uuid) -> Result<Option<CachedTarget>, CacheError> {
    let row: Option<CanonicalTargetRow> = sqlx::query_as(&format!(
        "SELECT {CANONICAL_TARGET_COLUMNS} FROM canonical_target WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(backend_err)?;
    match row {
        None => Ok(None),
        Some(r) => Ok(Some(assemble(pool, r).await?)),
    }
}

// ── Typeahead search ─────────────────────────────────────────────────────────

/// The best matching alias seen so far for one target during search dedup.
struct Best {
    alias: String,
    normalized_len: usize,
    rank: u8,
}

impl Best {
    /// A lower rank wins; ties break on the shorter matched alias.
    fn is_better_than(&self, other: &Self) -> bool {
        (self.rank, self.normalized_len) < (other.rank, other.normalized_len)
    }
}

async fn search(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, CacheError> {
    let q = normalize(query);
    if q.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    // Substring match covers prefix and exact; rank/dedup is decided in Rust
    // so the query stays a single indexed LIKE scan over `normalized`.
    // Escape LIKE metacharacters in the user query so they match literally.
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%{escaped}%");

    // Over-fetch a bounded multiple of `limit` so dedup across aliases still
    // fills the page; ordering by normalized length favours tighter matches
    // before the cap.
    let fetch_cap = i64::try_from(limit.saturating_mul(8).clamp(limit, 2000)).unwrap_or(2000);
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT target_id, alias, normalized
         FROM target_alias
         WHERE normalized LIKE ? ESCAPE '\\'
         ORDER BY LENGTH(normalized) ASC, normalized ASC
         LIMIT ?",
    )
    .bind(&pattern)
    .bind(fetch_cap)
    .fetch_all(pool)
    .await
    .map_err(backend_err)?;

    // Pick the best (lowest rank, then shortest alias) hit per target_id.
    let mut best_by_target: HashMap<String, Best> = HashMap::new();
    for (target_id, alias, normalized_alias) in rows {
        let rank = if normalized_alias == q {
            simbad_resolver_cache::RANK_EXACT
        } else if normalized_alias.starts_with(&q) {
            simbad_resolver_cache::RANK_PREFIX
        } else {
            simbad_resolver_cache::RANK_SUBSTRING
        };
        let candidate = Best { alias, normalized_len: normalized_alias.len(), rank };
        match best_by_target.entry(target_id) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if candidate.is_better_than(e.get()) {
                    e.insert(candidate);
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(candidate);
            }
        }
    }

    // Sort target ids by (rank, alias length, alias) for a stable best-first order.
    let mut ranked: Vec<(String, Best)> = best_by_target.into_iter().collect();
    ranked.sort_by(|(_, a), (_, b)| {
        (a.rank, a.normalized_len, a.alias.as_str()).cmp(&(
            b.rank,
            b.normalized_len,
            b.alias.as_str(),
        ))
    });
    ranked.truncate(limit);

    // Hydrate each winning target (load its full row + aliases).
    let mut hits = Vec::with_capacity(ranked.len());
    for (target_id, best) in ranked {
        let uuid = Uuid::parse_str(&target_id)
            .map_err(|e| CacheError::InvalidUuid(target_id.clone(), e))?;
        if let Some(target) = get_by_id(pool, uuid).await? {
            hits.push(SearchHit { target, matched_alias: best.alias, rank: best.rank });
        }
    }
    Ok(hits)
}

// ── Upsert ───────────────────────────────────────────────────────────────────

/// The row this identity should upsert into.
struct ExistingRow {
    id: String,
    source: TargetSource,
}

/// Find the row this identity should upsert into.
///
/// Dedup precedence (FR-005/FR-007): if `simbad_oid` is non-null and a row
/// with that oid exists, that row is the canonical one (keep its id so alias
/// FKs stay valid). Otherwise fall back to the designation-derived id.
async fn find_existing(
    conn: &mut SqliteConnection,
    identity: &ResolvedIdentity,
    derived: &str,
) -> Result<Option<ExistingRow>, CacheError> {
    if let Some(oid) = identity.simbad_oid {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT id, source FROM canonical_target WHERE simbad_oid = ?")
                .bind(oid)
                .fetch_optional(&mut *conn)
                .await
                .map_err(backend_err)?;
        if let Some((id, source)) = row {
            let source = TargetSource::from_wire(&source)
                .ok_or_else(|| CacheError::InvalidEnum(source.clone()))?;
            return Ok(Some(ExistingRow { id, source }));
        }
    }
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT id, source FROM canonical_target WHERE id = ?")
            .bind(derived)
            .fetch_optional(&mut *conn)
            .await
            .map_err(backend_err)?;
    match row {
        None => Ok(None),
        Some((id, source)) => {
            let source = TargetSource::from_wire(&source)
                .ok_or_else(|| CacheError::InvalidEnum(source.clone()))?;
            Ok(Some(ExistingRow { id, source }))
        }
    }
}

/// Replace all alias rows for `target_id` with `identity`'s aliases.
///
/// Aliases are rewritten wholesale (delete + insert) so a re-resolution that
/// adds/removes aliases stays consistent. The primary designation and (when
/// present) the common name are always written first — as `designation` /
/// `common_name` kind respectively — so `INSERT OR IGNORE` on the
/// `(target_id, normalized)` unique keeps their correct `kind` even if
/// `identity.aliases` happens to also list them.
async fn write_aliases(
    conn: &mut SqliteConnection,
    target_id: &str,
    identity: &ResolvedIdentity,
) -> Result<(), CacheError> {
    sqlx::query("DELETE FROM target_alias WHERE target_id = ?")
        .bind(target_id)
        .execute(&mut *conn)
        .await
        .map_err(backend_err)?;

    let mut to_write: Vec<ResolvedAlias> = Vec::with_capacity(identity.aliases.len() + 2);
    to_write.push(ResolvedAlias::new(identity.primary_designation.clone(), AliasKind::Designation));
    if let Some(name) = &identity.common_name {
        to_write.push(ResolvedAlias::new(name.clone(), AliasKind::CommonName));
    }
    to_write.extend(identity.aliases.iter().cloned());

    for a in &to_write {
        let alias_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO target_alias (id, target_id, alias, normalized, kind)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&alias_id)
        .bind(target_id)
        .bind(&a.alias)
        .bind(&a.normalized)
        .bind(a.kind.as_wire())
        .execute(&mut *conn)
        .await
        .map_err(backend_err)?;
    }
    Ok(())
}

fn now_rfc3339() -> String {
    // `OffsetDateTime::now_utc()` is always in Rfc3339's representable range,
    // so formatting cannot fail in practice.
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting of the current UTC time cannot fail")
}

async fn upsert(
    pool: &SqlitePool,
    identity: &ResolvedIdentity,
    namespace: &Uuid,
) -> Result<(Uuid, UpsertOutcome), CacheError> {
    let mut conn = pool.acquire().await.map_err(backend_err)?;
    let derived = target_id_from_designation(namespace, &identity.primary_designation).to_string();
    let existing = find_existing(&mut conn, identity, &derived).await?;
    let resolved_at = now_rfc3339();

    match existing {
        Some(row) if !identity.source.may_overwrite(row.source) => {
            // Existing row wins (a user-override is sticky vs resolved/seed).
            let id =
                Uuid::parse_str(&row.id).map_err(|e| CacheError::InvalidUuid(row.id.clone(), e))?;
            Ok((id, UpsertOutcome::SkippedUserOverride))
        }
        Some(row) => {
            // Update in place, keeping the existing id (preserve alias FKs).
            sqlx::query(
                "UPDATE canonical_target SET
                     simbad_oid          = ?,
                     primary_designation = ?,
                     object_type         = ?,
                     otype_raw           = ?,
                     ra_deg              = ?,
                     dec_deg             = ?,
                     source              = ?,
                     resolved_at         = ?
                 WHERE id = ?",
            )
            .bind(identity.simbad_oid)
            .bind(&identity.primary_designation)
            .bind(identity.object_type.as_wire())
            .bind(&identity.otype_raw)
            .bind(identity.ra_deg)
            .bind(identity.dec_deg)
            .bind(identity.source.as_wire())
            .bind(&resolved_at)
            .bind(&row.id)
            .execute(&mut *conn)
            .await
            .map_err(backend_err)?;
            write_aliases(&mut conn, &row.id, identity).await?;
            let id =
                Uuid::parse_str(&row.id).map_err(|e| CacheError::InvalidUuid(row.id.clone(), e))?;
            Ok((id, UpsertOutcome::Updated))
        }
        None => {
            sqlx::query(
                "INSERT INTO canonical_target
                     (id, simbad_oid, primary_designation, object_type, otype_raw, ra_deg, dec_deg, source, resolved_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&derived)
            .bind(identity.simbad_oid)
            .bind(&identity.primary_designation)
            .bind(identity.object_type.as_wire())
            .bind(&identity.otype_raw)
            .bind(identity.ra_deg)
            .bind(identity.dec_deg)
            .bind(identity.source.as_wire())
            .bind(&resolved_at)
            .execute(&mut *conn)
            .await
            .map_err(backend_err)?;
            write_aliases(&mut conn, &derived, identity).await?;
            let id = Uuid::parse_str(&derived)
                .map_err(|e| CacheError::InvalidUuid(derived.clone(), e))?;
            Ok((id, UpsertOutcome::Inserted))
        }
    }
}

#[async_trait::async_trait]
impl Cache for SqliteCache {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<CachedTarget>, CacheError> {
        get_by_id(&self.pool, id).await
    }

    async fn get_by_simbad_oid(&self, oid: i64) -> Result<Option<CachedTarget>, CacheError> {
        let row: Option<CanonicalTargetRow> = sqlx::query_as(&format!(
            "SELECT {CANONICAL_TARGET_COLUMNS} FROM canonical_target WHERE simbad_oid = ?"
        ))
        .bind(oid)
        .fetch_optional(&self.pool)
        .await
        .map_err(backend_err)?;
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(assemble(&self.pool, r).await?)),
        }
    }

    async fn get_by_normalized(
        &self,
        normalized: &str,
    ) -> Result<Option<CachedTarget>, CacheError> {
        let target_id: Option<(String,)> =
            sqlx::query_as("SELECT target_id FROM target_alias WHERE normalized = ? LIMIT 1")
                .bind(normalized)
                .fetch_optional(&self.pool)
                .await
                .map_err(backend_err)?;
        match target_id {
            None => Ok(None),
            Some((tid,)) => {
                let uuid =
                    Uuid::parse_str(&tid).map_err(|e| CacheError::InvalidUuid(tid.clone(), e))?;
                get_by_id(&self.pool, uuid).await
            }
        }
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, CacheError> {
        search(&self.pool, query, limit).await
    }

    async fn upsert(
        &self,
        identity: &ResolvedIdentity,
        namespace: &Uuid,
    ) -> Result<(Uuid, UpsertOutcome), CacheError> {
        upsert(&self.pool, identity, namespace).await
    }

    async fn add_user_alias(&self, target_id: Uuid, alias: &str) -> Result<bool, CacheError> {
        let normalized = normalize(alias);
        if normalized.is_empty() {
            return Ok(false);
        }
        let alias_id = Uuid::new_v4().to_string();
        let target_id_str = target_id.to_string();
        let result = sqlx::query(
            "INSERT OR IGNORE INTO target_alias (id, target_id, alias, normalized, kind)
             VALUES (?, ?, ?, ?, 'user')",
        )
        .bind(&alias_id)
        .bind(&target_id_str)
        .bind(alias)
        .bind(&normalized)
        .execute(&self.pool)
        .await
        .map_err(backend_err)?;
        Ok(result.rows_affected() > 0)
    }

    async fn remove_user_alias(&self, alias_id: &str) -> Result<bool, CacheError> {
        let result = sqlx::query("DELETE FROM target_alias WHERE id = ? AND kind = 'user'")
            .bind(alias_id)
            .execute(&self.pool)
            .await
            .map_err(backend_err)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<CachedTarget>, CacheError> {
        let rows: Vec<CanonicalTargetRow> = sqlx::query_as(&format!(
            "SELECT {CANONICAL_TARGET_COLUMNS} FROM canonical_target ORDER BY primary_designation ASC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(backend_err)?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Batch-load every alias in one query (avoids N+1).
        let alias_rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT target_id, alias, normalized, kind FROM target_alias ORDER BY target_id ASC, alias ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(backend_err)?;

        let mut aliases_by_id: HashMap<String, Vec<ResolvedAlias>> = HashMap::new();
        for (target_id, alias, normalized, kind) in alias_rows {
            aliases_by_id.entry(target_id).or_default().push(ResolvedAlias {
                alias,
                normalized,
                kind: AliasKind::from_wire(&kind),
            });
        }

        rows.into_iter()
            .map(
                |(
                    id_str,
                    simbad_oid,
                    primary_designation,
                    object_type,
                    otype_raw,
                    ra_deg,
                    dec_deg,
                    source,
                    resolved_at,
                )| {
                    let id = Uuid::parse_str(&id_str)
                        .map_err(|e| CacheError::InvalidUuid(id_str.clone(), e))?;
                    let source = TargetSource::from_wire(&source)
                        .ok_or_else(|| CacheError::InvalidEnum(source.clone()))?;
                    let aliases = aliases_by_id.remove(&id_str).unwrap_or_default();
                    let common_name = aliases
                        .iter()
                        .find(|a| a.kind == AliasKind::CommonName)
                        .map(|a| a.alias.clone());
                    Ok(CachedTarget {
                        id,
                        simbad_oid,
                        primary_designation,
                        common_name,
                        object_type: ObjectType::from_wire(&object_type),
                        otype_raw,
                        ra_deg,
                        dec_deg,
                        source,
                        resolved_at,
                        aliases,
                    })
                },
            )
            .collect()
    }
}
