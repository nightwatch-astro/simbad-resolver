//! Restart-durability tests for the file-backed SQLite store.
//!
//! `tests/cache.rs` and `tests/queue.rs` use `SqliteStore::in_memory()`, which
//! cannot prove FR-011 ("pending items survive a restart") because an in-memory
//! database vanishes when its connection closes. These tests open a real
//! on-disk database with [`SqliteStore::open`], write to it, drop the store to
//! close the pool, then reopen the SAME file and assert the state persisted.

use std::sync::atomic::{AtomicU32, Ordering};

use simbad_resolver_cache::{Cache, Queue};
use simbad_resolver_cache_sqlite::SqliteStore;
use simbad_resolver_core::identity::namespace;
use simbad_resolver_core::{AliasKind, ObjectType, ResolvedAlias, ResolvedIdentity, TargetSource};

/// A temp-dir database path unique to this process + call, so parallel tests
/// never share a file.
fn unique_db_path() -> std::path::PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("simbad-resolver-restart-{}-{n}.db", std::process::id()))
}

fn m31() -> ResolvedIdentity {
    ResolvedIdentity {
        simbad_oid: Some(1_575_544),
        primary_designation: "M 31".to_owned(),
        common_name: Some("Andromeda Galaxy".to_owned()),
        object_type: ObjectType::Galaxy,
        otype_raw: "G".to_owned(),
        ra_deg: 10.684_708,
        dec_deg: 41.268_75,
        aliases: vec![
            ResolvedAlias::new("M 31", AliasKind::Designation),
            ResolvedAlias::new("NGC 224", AliasKind::Designation),
        ],
        source: TargetSource::Resolved,
    }
}

#[tokio::test]
async fn cache_and_queue_state_survive_a_reopen() {
    let path = unique_db_path();
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let ns = namespace("simbad-resolver-cache-sqlite.restart");

    // First "run": open a fresh db, write a queued item + a cached target.
    {
        let store = SqliteStore::open(&url).await.expect("open new database");
        store.queue().enqueue("img-1", "M 31").await.unwrap();
        store.cache().upsert(&m31(), &ns).await.unwrap();
        assert_eq!(store.queue().pending_count().await.unwrap(), 1);
        store.pool().close().await; // deterministic flush + close
    }

    // Second "run": reopen the SAME file; committed state must still be there.
    {
        let store = SqliteStore::open(&url).await.expect("reopen existing database");

        let pending = store.queue().get("img-1").await.unwrap().expect("pending item persisted");
        assert_eq!(pending.query, "M 31");
        assert_eq!(store.queue().pending_count().await.unwrap(), 1);

        let cached = store
            .cache()
            .get_by_simbad_oid(1_575_544)
            .await
            .unwrap()
            .expect("cached target persisted");
        assert_eq!(cached.primary_designation, "M 31");
        assert!(cached.aliases.iter().any(|a| a.alias == "NGC 224"));

        store.pool().close().await;
    }

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn open_is_idempotent_and_migrations_are_reappliable() {
    // Opening the same file twice (fresh, then already-migrated) must both
    // succeed — the migrations are guarded and re-runnable.
    let path = unique_db_path();
    let url = format!("sqlite://{}?mode=rwc", path.display());

    let first = SqliteStore::open(&url).await.expect("first open runs migrations");
    first.pool().close().await;

    let second = SqliteStore::open(&url).await.expect("second open re-runs cleanly");
    assert!(second.cache().list().await.unwrap().is_empty());
    second.pool().close().await;

    let _ = std::fs::remove_file(&path);
}
