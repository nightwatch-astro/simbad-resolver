# Feature Specification: SIMBAD Target Resolution (simbad-resolver)

**Feature Branch**: `001-simbad-target-resolution`

**Created**: 2026-07-11

**Status**: Draft

**Input**: Extract and generalize the target-resolution subsystem of the astro-plan app (`targeting` / `targeting_resolver`) into a standalone, embeddable Rust library `simbad-resolver`, decoupled from that app's database schema, identity namespace, HTTP branding, bundled seed, and product surfaces.

## Overview

`simbad-resolver` turns an astronomical **name or catalog designation** (e.g. `M31`, `NGC 224`, `Andromeda Galaxy`, `C 14`) — or a **sky position** — into a single **canonical target identity**: ICRS J2000 coordinates, a closed object-type classification, the full alias/designation set, and a stable identifier. Resolution happens on demand against **SIMBAD** and is backed by a **pluggable cache** so repeated and offline lookups are instant and never re-query the network.

The library is **upstream** in its ecosystem: it answers *"what is this object?"* and produces identities. It is embeddable in any Rust project (planners, ingest pipelines, plate-solve post-processing, catalog browsers) that needs authoritative object identity without owning SIMBAD querying, deduplication, and caching itself.

## Clarifications

### Session 2026-07-11

- Q: Are resolver settings (online-enabled, endpoint, timeout, User-Agent) caller-owned config or persisted like the origin app? → A: **Caller-owned constructor config**; the library does not persist settings and the durable store has no settings table.
- Q: How does the Sesame (broad-coverage) resolver relate to the TAP (precise) resolver, given Sesame's coarser raw output? → A: **Standalone by default, with optional enrichment** — the Sesame resolver may enrich its result via a caller-supplied resolver, but does not depend on the TAP resolver.
- Q: How durable is the async batch/pending queue? → A: **A pluggable `Queue` abstraction**, with both an in-memory implementation and a durable (cache-backed) implementation shipped — mirroring the cache, which ships a memory backend and a durable (SQLite) backend.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Resolve a name to a canonical identity (Priority: P1)

A developer integrates the library and passes a free-form designation or common name; the library returns one canonical identity with real ICRS J2000 coordinates, an object-type classification, and every known alias for that physical object — or a clear, typed "unresolved" outcome. It never invents a coordinate.

**Why this priority**: This is the core value; without it nothing else matters. It is a viable MVP on its own.

**Independent Test**: Resolve `M 31` and assert the result carries valid coordinates, `galaxy` type, and aliases including `NGC 224` and `Andromeda Galaxy`; resolve a garbled string and assert an unresolved/not-found outcome with no fabricated coordinate.

**Acceptance Scenarios**:

1. **Given** a valid designation (`NGC 224`), **When** resolved online, **Then** one identity is returned with ICRS J2000 RA/Dec, object type, the full alias set, a curated common name when one exists, and a stable id.
2. **Given** a query that maps to several distinct physical objects, **When** resolved, **Then** the result is an **ambiguous** outcome (the caller is not handed a guess).
3. **Given** an unknown/garbled query, **When** resolved, **Then** the result is **not-found** and no coordinate is invented.
4. **Given** the same physical object referred to by different aliases (`M 31` vs `NGC 224`), **When** each is resolved, **Then** both collapse to **one** canonical identity (deduplicated by physical-object identity, never split by alias/catalog).

---

### User Story 2 - Offline-first search and repeat lookups (Priority: P1)

A developer performs as-you-type lookups and repeat resolutions. Anything previously resolved (or provided via the cache) is found instantly with no network call; unknown queries fall through to online resolution at most once, then are cached.

**Why this priority**: Interactive latency and offline resilience are as important as the resolve itself, and they are what make the library usable in a UI or a batch pipeline.

**Independent Test**: Populate the cache, disconnect the network, and assert typeahead search over cached aliases returns ranked results quickly; assert a second resolve of the same object performs no network call.

**Acceptance Scenarios**:

1. **Given** a populated cache, **When** searching by a partial alias with no network, **Then** matching canonical targets are returned ranked exact > prefix > substring, deduplicated to one entry per target, capped to a caller-supplied limit.
2. **Given** an object already in the cache, **When** it is resolved again, **Then** the cached identity is returned with **no** network request (resolve-at-most-once).
3. **Given** the network is unreachable or resolution is disabled, **When** a cache-miss query is resolved, **Then** the outcome is an explicit **offline/unresolved** state, and cached/known objects still resolve normally.

---

### User Story 3 - Choose a resolver and resolve by position (Priority: P2)

A developer selects which resolution backend to use for a given need — a precise, structured resolver or a broad-coverage aggregating resolver — and can also resolve a **sky position** to the nearest known object(s).

**Why this priority**: Different consumers have different needs (precision vs. name coverage; name lookup vs. position lookup). Offering both first-class resolvers and a position query broadens applicability without forcing one choice.

**Independent Test**: Resolve the same name through each resolver behind a common interface and assert both return a canonical identity; resolve a position with a radius and assert the nearest object(s) are returned with their angular separation.

**Acceptance Scenarios**:

1. **Given** two interchangeable resolvers behind one interface, **When** a caller swaps one for the other, **Then** no change to calling code is required and both return canonical identities.
2. **Given** a sky position and a search radius, **When** a position resolve is requested, **Then** the nearest known object(s) within the radius are returned, each with its angular separation, ordered nearest-first.
3. **Given** documentation, **When** a developer chooses a resolver, **Then** the docs state clearly when each resolver is preferable (precision/structured data vs. broader name coverage).

---

### User Story 4 - Batch-resolve many identifiers politely (Priority: P2)

A developer has many identifiers to resolve (e.g. an import) and wants them resolved in the background without blocking, without re-resolving what is cached, and without overwhelming the upstream service. Transient failures are retried later; genuine misses are recorded as unresolved.

**Why this priority**: Bulk resolution is a common real-world use and the upstream service expects polite, rate-limited access; getting the transient-vs-miss distinction right prevents both data loss and abuse.

**Independent Test**: Enqueue a mix of cached, resolvable, unknown, and transiently-failing identifiers keyed by opaque ids; drain the queue and assert cached items are inline, resolvable items are resolved once, unknown items become unresolved, and transiently-failing items remain pending for retry.

**Acceptance Scenarios**:

1. **Given** a batch of identifiers keyed by opaque caller ids, **When** the background drain runs, **Then** each is resolved cache-first then online (when enabled), within a bounded concurrency.
2. **Given** a transient failure (network/timeout/disabled) for an item, **When** the drain processes it, **Then** it stays **pending** for a later retry and its attempt budget is **not** consumed.
3. **Given** a genuine content miss (not-found/ambiguous), **When** the drain processes it, **Then** it becomes **unresolved** and its attempt count increases.

---

### User Story 5 - Manual override with sticky precedence (Priority: P2)

When automatic resolution is wrong or a curator knows better, a developer binds a query/alias to a chosen canonical target as an authoritative override. That override persists and is never silently overwritten by later automatic resolutions.

**Why this priority**: Real catalogs have edge cases and human corrections; without a durable override, automation would repeatedly clobber known-good corrections.

**Independent Test**: Override an object, then re-resolve it online with a different result and assert the override is retained; assert precedence ordering (override > resolved > seed) governs all writes.

**Acceptance Scenarios**:

1. **Given** an existing automatic (resolved) identity, **When** a user override is applied, **Then** the override becomes the stored identity.
2. **Given** a stored user override, **When** a later automatic resolution produces a different result for the same object, **Then** the override is **retained** (not overwritten).
3. **Given** two writes of differing provenance to the same object, **When** they are reconciled, **Then** the higher-precedence source (override > resolved > seed) wins.

---

### User Story 6 - Pluggable cache backend (Priority: P3)

A developer chooses where identities are stored — an in-memory store for tests/ephemeral use, a durable local database, or a custom backend — without changing resolve/orchestration code.

**Why this priority**: Different deployments have different persistence needs; a fixed backend would exclude valid use cases. It is P3 because a default backend covers most needs.

**Independent Test**: Run the same orchestration test suite against the in-memory backend and the durable backend and assert identical observable behavior.

**Acceptance Scenarios**:

1. **Given** a cache abstraction with multiple implementations, **When** a caller substitutes one backend for another, **Then** resolve/search/override behavior is unchanged.
2. **Given** a custom backend implementing the cache abstraction, **When** it is supplied, **Then** the library uses it with no other code changes.

---

### User Story 7 - Embed with caller-owned identity and etiquette (Priority: P3)

A developer embeds the library into an existing application and needs stable ids that match their existing data, plus polite, identifying network etiquette that names *their* application.

**Why this priority**: Reuse in an existing system requires id-stability and correct attribution; hardcoded identity/branding would break continuity and violate service etiquette. P3 because sensible neutral defaults work out of the box.

**Independent Test**: Configure a caller-supplied id namespace and assert derived ids match the caller's existing ids for the same designation; configure a User-Agent and assert outbound requests carry it.

**Acceptance Scenarios**:

1. **Given** a caller-supplied identity namespace, **When** ids are derived from designations, **Then** they are deterministic and match the caller's existing ids for the same input.
2. **Given** a caller-supplied User-Agent and endpoint/timeout, **When** the library queries the service, **Then** those values are used (with neutral, identifying defaults when unset).

---

### Edge Cases

- **Out-of-range coordinates** returned by the upstream service degrade the affected record to not-found; a bad coordinate is never written to the cache.
- **Oversized or malformed upstream responses** are rejected safely (bounded body size; error bodies detected even when delivered with a success status) rather than consuming unbounded memory or being parsed as data.
- **Whitespace-padded / quoted upstream identifiers** are normalized so a single-spaced query matches a padded stored form and vice-versa.
- **Caldwell designations** (`C n`, `Caldwell n`) — which the upstream service does not recognize as identifiers — are translated to their standard catalog designation, resolved, and the original `C n` bound as an alias. Entries with no single resolvable designation (e.g. C99, the Coalsack) resolve to not-found rather than a guess.
- **Object types outside the known set** classify as a catch-all `other` (an identity is never dropped for lack of a type), while the raw upstream type string remains available.
- **Ambiguous common names** with multiple curated names resolve deterministically (a stable choice, not order-dependent).
- **Re-seeding / re-loading** the cache is idempotent and never duplicates a physical object or clobbers a sticky override.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST resolve a designation or common name to exactly one canonical identity carrying ICRS J2000 coordinates, an object-type classification, the full alias/designation set, a curated common name when one exists, and a stable id.
- **FR-002**: The system MUST offer more than one interchangeable resolution backend behind a single resolver interface — a precise/structured resolver and a broad-coverage aggregating resolver — selectable by the caller, with documentation of when each is preferable. The broad-coverage resolver MAY optionally enrich its (potentially coarser) result via a caller-supplied resolver, but MUST NOT hard-depend on the precise resolver.
- **FR-003**: The system MUST consult the cache before the network, persist every resolution, and never re-query an already-cached object (resolve-at-most-once).
- **FR-004**: The system MUST provide the cache as a pluggable abstraction with at least an in-memory implementation and a durable local implementation, and MUST allow a caller-supplied implementation, without changes to resolve/orchestration logic.
- **FR-005**: The system MUST deduplicate all aliases of one physical object onto a single canonical identity (by the upstream physical-object identifier, with a designation-derived deterministic id as fallback when that identifier is unknown); it MUST NOT split one object across aliases or catalogs.
- **FR-006**: The system MUST enforce source precedence — user-override > resolved > seed — such that a user override is sticky and a later resolved/seed write MUST NOT overwrite it.
- **FR-007**: Callers MUST be able to bind a query/alias to a chosen canonical identity as an authoritative, persisted user override.
- **FR-008**: The system MUST NOT fabricate coordinates or identity: an unknown query yields not-found, a query matching several distinct physical objects yields ambiguous, and an offline/timeout/disabled condition degrades to the cache with an explicit offline/unresolved outcome.
- **FR-009**: The system MUST resolve a sky position with a search radius to the nearest known object(s), each with its angular separation, ordered nearest-first.
- **FR-010**: The system MUST provide a local, network-free typeahead search over cached aliases, ranked exact > prefix > substring, deduplicated to one entry per target, and capped to a caller-supplied limit.
- **FR-011**: The system MUST provide an asynchronous batch resolver that accepts identifiers keyed by opaque caller ids, drains them cache-first then online within a bounded concurrency, keeps **transient** failures pending for retry without consuming an attempt budget, and marks **content** misses unresolved. The pending items MUST be held behind a pluggable **queue abstraction**; the library MUST ship both an in-memory queue and a durable (cache-backed) queue whose pending items survive a restart.
- **FR-012**: The system MUST support disabling online resolution, in which case only cache/seed lookups are performed.
- **FR-013**: The system MUST derive stable ids from a **caller-configurable** identity namespace so that ids are deterministic and can match a caller's existing ids for the same designation.
- **FR-014**: The system MUST expose caller-configurable network etiquette — endpoint, request timeout, and an identifying User-Agent — with neutral, identifying defaults. These, together with the online-resolution toggle (FR-012), are caller-owned constructor configuration; the library MUST NOT persist them.
- **FR-015**: The system MUST translate Caldwell designations to their standard resolvable catalog designation, resolve that, and bind the original Caldwell designation as an alias; Caldwell entries with no single resolvable designation yield not-found.
- **FR-016**: The system MUST map upstream object-type codes to a closed, documented classification via a total mapping (unknown/empty → a catch-all `other`) while preserving the raw upstream type string.
- **FR-017**: The system MUST treat all upstream responses defensively: bound response body size, detect error responses even when delivered with a success status, and normalize whitespace-padded/quoted identifiers before use.
- **FR-018**: The system MUST NOT perform fuzzy/probabilistic matching for resolution; matching is exact after a documented normalization (case-folding, punctuation stripping, whitespace collapse, catalog-prefix expansion).
- **FR-019**: The library core (identity types, normalization, id derivation) MUST be usable with no asynchronous, network, or database dependencies, so purely-typed consumers avoid that weight.
- **FR-020**: The library MUST document its ecosystem boundary and interaction with the shared coordinate-primitives crate and the downstream position-matching crate (see Assumptions), and MUST NOT itself implement local nearest-neighbour / field-of-view geometry.

### Key Entities *(include if feature involves data)*

- **Canonical Target**: one physical astronomical object — its stable id, the upstream physical-object identifier (deduplication key when known), a primary/display designation, an object-type classification, ICRS J2000 coordinates, a provenance/source, and a resolved-at timestamp.
- **Target Alias**: one alternate designation or curated common name attached to a canonical target — the verbatim form, a normalized matching form, and a kind (designation, common name, or user-added).
- **Resolved Identity**: the value a resolver returns before persistence — the canonical target's fields plus its full alias set — never carrying a fabricated coordinate.
- **Resolver Configuration** *(caller-owned, not persisted)*: whether online resolution is enabled, the upstream endpoint, the request timeout, and the identifying User-Agent — supplied by the caller at construction; the library does not store these.
- **Resolver (abstraction)**: the interchangeable resolution backend; concrete backends are a precise/structured resolver, a broad-coverage aggregating resolver (optionally enriched via a supplied resolver), an always-offline resolver, and a test double.
- **Cache (abstraction)**: the interchangeable identity store supporting get-by-id/physical-id/normalized-alias, ranked search, precedence-aware upsert with deduplication, alias add/remove, and override set/clear; shipped in in-memory and durable (SQLite) forms, plus caller-supplied.
- **Queue (abstraction)**: the interchangeable pending-work store for the batch resolver — enqueue, claim/drain, mark resolved/unresolved/pending, and attempt tracking; shipped in in-memory and durable (cache-backed) forms.
- **Pending Resolution**: one queued batch item — an opaque caller id, the raw query, its state (pending / resolved / unresolved), an attempt count, and the resolved canonical-target id when resolved.
- **Object Type**: the closed classification (galaxy, planetary nebula, emission nebula, reflection nebula, dark nebula, open cluster, globular cluster, supernova remnant, galaxy cluster, double star, asterism, other).
- **Source / Provenance**: the origin of a stored identity (seed, resolved, user-override) governing write precedence.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Typeahead search over a populated cache returns ranked results in under 100 ms with no network access.
- **SC-002**: Any object known to the upstream service resolves when online, with no fixed-catalog ceiling, and is resolved at most once per physical object.
- **SC-003**: 100% of aliases of the same physical object group under one canonical identity (zero alias-based splits) across a representative alias set.
- **SC-004**: With the network disabled, cache search and resolution of known objects continue to succeed, and every unknown query is clearly reported as unresolved — zero silent mis-assignments and zero fabricated coordinates.
- **SC-005**: A consumer depending only on the identity/normalization core compiles it with zero asynchronous, network, or database dependencies.
- **SC-006**: Substituting one cache backend for another (in-memory ↔ durable ↔ custom) requires no change to resolve/orchestration code, verified by running one behavior suite unchanged against each backend.
- **SC-007**: A caller-supplied identity namespace produces ids identical to a reference derivation for the same designation, demonstrating drop-in id continuity for an existing consumer.

## Assumptions

- **Upstream service**: SIMBAD (operated at CDS, Strasbourg) is the resolution source; consumers displaying resolved data credit CDS and send an identifying User-Agent, which the library requires callers to be able to set. A broad-coverage resolver additionally aggregates NED/VizieR via the upstream Sesame service.
- **Coordinates**: coordinates are ICRS J2000 in decimal degrees. The library will adopt the shared `astro-angle` coordinate-primitive crate (`Equatorial`, sexagesimal) as its coordinate type once that crate exists; until then it exposes decimal-degree values behind a conversion seam. This is a forward-compatible detail, not a v1 blocker.
- **Ecosystem role**: the library is upstream (name → identity; produces objects). The downstream `target-match` crate (formerly `astro-target-id`) consumes the library's output to rank candidates by angular separation within a frame's field of view; the optics/FOV geometry lives there, not here. The library's position resolve (cone search) is a complementary upstream-service query, not a duplicate of `target-match`'s local ranking. Objects are never passed *into* the resolver.
- **Seed data is out of scope for v1**: the core install ships no bundled catalog; curated seed catalogs (Messier/Caldwell/NGC/IC/…) will ship as separate packages later. The cache and precedence model already accommodate seed-sourced entries.
- **No product surfaces**: user interface, IPC/desktop integration, settings panels, attribution rendering, and wiring to any specific application's image/ingest tables are out of scope — those belong to consumers.
- **Schema scope**: the durable cache owns only the canonical-target and alias data (plus a pending-resolution table for the durable queue implementation); resolver settings are caller-owned config, not persisted; application-specific columns and tables from the origin app are not carried over. The default durable backend is SQLite; a pure-Rust embedded alternative (e.g. `redb`) is a possible future backend behind the same cache/queue abstractions.
- **Extraction fidelity**: the resolution logic, normalization, deduplication, precedence, Caldwell translation, object-type mapping, and defensive upstream-response handling are carried over from the proven origin implementation; the primary changes are decoupling (configurable identity/etiquette, owned schema, pluggable cache) and additions (a second first-class resolver, position/cone search, the async batch resolver).
