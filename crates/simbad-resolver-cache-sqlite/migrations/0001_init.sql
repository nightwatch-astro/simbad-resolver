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
