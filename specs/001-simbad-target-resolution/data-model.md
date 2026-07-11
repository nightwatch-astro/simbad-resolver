# Data Model: SIMBAD Target Resolution

## Rust types (crate `simbad-resolver-core`)

### `ObjectType` (closed enum, `snake_case` wire form)

`Galaxy, PlanetaryNebula, EmissionNebula, ReflectionNebula, DarkNebula, OpenCluster, GlobularCluster, SupernovaRemnant, GalaxyCluster, DoubleStar, Asterism, Other`.

- `map_otype(&str) -> ObjectType` — total; unknown/empty → `Other`.
- `as_wire()` / `from_wire()` — DB/serialization strings.
- Resolved identities additionally carry the **raw** otype string (`otype_raw: String`) for consumers needing finer types (R10).

### `TargetSource` (provenance / precedence)

`Seed (0) < Resolved (1) < UserOverride (2)`. Wire: `seed`, `resolved`, `user-override` (hyphenated).

- `precedence(self) -> u8`; `may_overwrite(self, existing) -> bool` = `self.precedence() >= existing.precedence()`.

### `AliasKind`

`Designation`, `CommonName`, `User`. Wire: `designation`, `common_name`, `user`.

### `ResolvedAlias`

| Field | Type | Notes |
|---|---|---|
| `alias` | `String` | verbatim designation / common name |
| `normalized` | `String` | normalized matching form (see normalize pipeline) |
| `kind` | `AliasKind` | |

`ResolvedAlias::new(alias, kind)` computes `normalized` via `normalize`.

### `ResolvedIdentity` (what a `Resolver` returns; never fabricated)

| Field | Type | Notes |
|---|---|---|
| `simbad_oid` | `Option<i64>` | physical-object id; dedup key when `Some` |
| `primary_designation` | `String` | canonical display designation |
| `common_name` | `Option<String>` | curated `NAME …` common name |
| `object_type` | `ObjectType` | mapped |
| `otype_raw` | `String` | raw SIMBAD otype (R10) |
| `ra_deg` | `f64` | ICRS J2000, `[0,360)` |
| `dec_deg` | `f64` | ICRS J2000, `[-90,90]` |
| `aliases` | `Vec<ResolvedAlias>` | designations + common names |
| `source` | `TargetSource` | provenance |

Coordinates are validated at parse time; out-of-range degrades to not-found (never stored). `astro-angle` `Equatorial` replaces `(ra_deg, dec_deg)` later (R11) behind a conversion seam.

### `ResolveError` (typed; `thiserror`)

`Network(String)`, `Timeout(u64)`, `Disabled`, `NotFound(String)`, `Ambiguous { query, count }`, `Parse(String)`. Transient = `Network`/`Timeout`/`Disabled`; content miss = `NotFound`/`Ambiguous`/`Parse`.

### `CachedTarget` (read model from a `Cache`)

`ResolvedIdentity` fields **plus** persisted `id: Uuid`, `resolved_at: String` (RFC3339). (No `display_alias` — that was app-specific.)

### `PositionMatch` (cone-search result)

| Field | Type |
|---|---|
| `identity` | `ResolvedIdentity` |
| `separation_deg` | `f64` |

### `PendingItem` (batch/queue read model)

| Field | Type | Notes |
|---|---|---|
| `id` | `String` | opaque caller id (queue key) |
| `query` | `String` | raw identifier to resolve |
| `state` | `PendingState` | `Pending` / `Resolved` / `Unresolved` |
| `attempts` | `i64` | incremented only on content misses |
| `target_id` | `Option<Uuid>` | set when resolved |

## Identity derivation (`simbad-resolver-core::identity`)

- `namespace(seed: &str) -> Uuid` = `UUIDv5(NAMESPACE_DNS, seed)` — **caller-supplied** seed.
- `target_id_from_designation(ns: &Uuid, designation: &str) -> Uuid` = `UUIDv5(ns, designation)`.
- Dedup precedence (FR-005): match by `simbad_oid` when `Some`; else by the designation-derived id.

## Normalization pipeline (`simbad-resolver-core::normalize`)

`normalize(&str) -> String`: NFKC → ASCII lowercase → strip non-alphanumeric/whitespace → collapse whitespace → catalog-prefix expansion (`m31`→`m 31`, `ngc224`→`ngc 224`, `ic…`, `sh2…`, `b…`, `vdb…`, `ldn…`, `lbn…`, `mel…`, `c…`, `arp…`, `caldwell…`). `tokenize(&str) -> Vec<&str>` (sorted, deduped).

## Persistent schema (crate `simbad-resolver-cache-sqlite`, `migrations/0001_init.sql`)

Adapted from astro-plan migration 0031, minus all app-specific columns/tables (`display_alias`, `constellation`, `magnitude`, `notes`, `favourites`, `ingest_resolution→file_record`, `resolver_settings`). Adds a generic `pending_resolution` table for the durable queue.

```sql
CREATE TABLE IF NOT EXISTS canonical_target (
    id                  TEXT    NOT NULL PRIMARY KEY,   -- UUID (v5 derived or v-from-oid)
    simbad_oid          INTEGER,                        -- physical-object id; UNIQUE when non-null
    primary_designation TEXT    NOT NULL,
    object_type         TEXT    NOT NULL,               -- ObjectType wire (snake_case)
    otype_raw           TEXT    NOT NULL DEFAULT '',    -- raw SIMBAD otype (R10)
    ra_deg              REAL    NOT NULL CHECK (ra_deg  >= 0   AND ra_deg  < 360),
    dec_deg             REAL    NOT NULL CHECK (dec_deg >= -90 AND dec_deg <= 90),
    source              TEXT    NOT NULL CHECK (source IN ('seed','resolved','user-override')),
    resolved_at         TEXT    NOT NULL                -- RFC 3339 UTC
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_canonical_target_simbad_oid
    ON canonical_target(simbad_oid) WHERE simbad_oid IS NOT NULL;

CREATE TABLE IF NOT EXISTS target_alias (
    id          TEXT NOT NULL PRIMARY KEY,
    target_id   TEXT NOT NULL REFERENCES canonical_target(id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    normalized  TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('designation','common_name','user')),
    UNIQUE (target_id, normalized)
);
CREATE INDEX IF NOT EXISTS idx_target_alias_normalized ON target_alias(normalized);

CREATE TABLE IF NOT EXISTS pending_resolution (
    id          TEXT NOT NULL PRIMARY KEY,   -- opaque caller id
    query       TEXT NOT NULL,
    state       TEXT NOT NULL CHECK (state IN ('pending','resolved','unresolved')),
    target_id   TEXT REFERENCES canonical_target(id),
    attempts    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_pending_resolution_pending
    ON pending_resolution(state) WHERE state = 'pending';
```

## Lifecycle / state transitions

- **Canonical target**: inserted on first resolve/seed; updated in place on re-resolve iff incoming source precedence ≥ existing (R7); aliases rewritten wholesale on update. A `user-override` row is sticky.
- **Pending item**: `Pending` → `Resolved` (content hit, `target_id` set) | `Unresolved` (content miss, `attempts++`); a transient failure leaves it `Pending` with `attempts` unchanged (FR-011).
