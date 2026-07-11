# Tasks: SIMBAD Target Resolution (simbad-resolver)

**Note on process**: implementation was driven directly, crate-by-crate in
dependency order (with parallel worktree subagents for the independent leaf
crates), rather than through the full SpecKit agent-assign DAG — see
`DECISIONS.md`. This file records the task breakdown and its (completed) status
for traceability. All tasks below are **done**; the workspace builds, 149 tests
pass, `clippy -D warnings` is clean, and `fmt --check`/`doc` are clean.

## Phase A — Foundation

- [X] T001 Workspace scaffold: 8 `simbad-resolver-*` member crates, shared deps + lints (`Cargo.toml`).
- [X] T002 `simbad-resolver-core`: `ObjectType`+`map_otype` (+`otype_raw`), `TargetSource` precedence, `AliasKind`, `ResolvedAlias`/`ResolvedIdentity`, `PositionMatch`, `ResolveError` (+`is_transient`), `normalize`/`tokenize`, `identity` (configurable namespace), `SimbadConfig`, `wire` helpers, `Resolver`/`PositionResolver` traits, `OfflineResolver`/`FakeResolver`. (53 tests; zero async/net/db deps — SC-005.)
- [X] T003 `simbad-resolver-caldwell`: C1–C109 map + `caldwell_to_designation` + `parse_caldwell_number`. (7 tests.)
- [X] T004 `simbad-resolver-cache`: `Cache` + `Queue` async traits, `CachedTarget`/`SearchHit`/`UpsertOutcome`/`PendingItem`/`PendingState`, `CacheError`/`QueueError`.

## Phase B — Resolver backends (FR-002, FR-009)

- [X] T005 `simbad-resolver-tap`: `SimbadTapResolver` — 2-round-trip name resolve + cone search; `reqwest` query-builder; defensive parsing (body cap, VOTable-error, TSV). (11 unit + 3 ignored live.)
- [X] T006 `simbad-resolver-sesame`: `SimbadSesameResolver` — Sesame name resolve, standalone + optional caller-supplied enrichment (no `-tap` dep). (14 unit + 1 ignored live. Sesame fixture NEEDS LIVE VERIFICATION — see DECISIONS.md.)

## Phase C — Cache/Queue backends (FR-003–FR-007, FR-010–FR-011, SC-001/006)

- [X] T007 `simbad-resolver-cache-memory`: `MemoryCache` + `MemoryQueue` (dashmap): dedup+precedence upsert, ranked search, user aliases, queue state machine. (26 tests.)
- [X] T008 `simbad-resolver-cache-sqlite`: `SqliteStore`/`SqliteCache`/`SqliteQueue` (sqlx) + `migrations/0001_init.sql` (canonical_target/target_alias/pending_resolution). (28 tests.)

## Phase D — Facade + orchestration (FR-001, FR-008, FR-011–FR-016, FR-020)

- [X] T009 `simbad-resolver`: `SimbadResolver<R,C>` cache-first resolve (+Caldwell translation, degrade, never-fabricate), `apply_override` (sticky), `search`; `BatchResolver<R,C,Q>` async drain (transient-vs-miss); `ResolverConfig`; feature-gated backend re-exports; ecosystem docs (astro-angle/target-match).
- [X] T010 Integration tests: resolve+dedup, unknown/ambiguous/offline, online-disabled, sticky override, Caldwell, batch drain, SQLite cross-backend (SC-006). (9 tests + doctest.)

## Phase E — Verification & delivery

- [X] T011 Workspace `cargo test`/`clippy -D warnings`/`fmt --check`/`doc` green.
- [X] T012 CI (fmt·clippy·test·doc), justfile, ADR-0001, README/AGENTS ecosystem docs, apm.yml.
- [ ] T013 (Deferred/optional) Live-network verification of TAP + Sesame parsers (`-- --ignored`) on a networked host; refresh Sesame fixture if needed.
- [ ] T014 (Deferred/optional) `astro-angle` adoption for coordinates; `simbad-seed` + `simbad-seed-builder` as separate packages; crates.io publish (release-please).
