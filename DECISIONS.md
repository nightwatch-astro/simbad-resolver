# Decisions, Ambiguities & Inputs Needed

A running log of autonomous decisions, open ambiguities, and anything that needs
the maintainer's input. Newest at the top of each section.

## Inputs needed from the maintainer

- **crates.io publish / naming**: the `simbad-resolver-*` names are not yet
  reserved on crates.io. Confirm before first `cargo publish`. (No action taken.)
- **`astro-angle` / `target-match`**: both are greenfield siblings. This crate
  uses a decimal-degree seam now and will adopt `astro_angle::Equatorial` once
  that crate exists. No blocking dependency.
- **Dual-licensing**: currently Apache-2.0 only. Rust convention is dual
  `MIT OR Apache-2.0`; say the word and I'll add the MIT file + update metadata.

## Open ambiguities (assumed a default; flag if wrong)

- **MSRV** assumed `1.82`. Adjust if you target older.
- **Cone-search radius units**: degrees (matches ICRS decimal-degree convention).
- **Sesame XML format (NEEDS LIVE VERIFICATION)**: `simbad-resolver-sesame`
  parses a **hand-built** `-oxp` (`SNV`) fixture based on the documented schema,
  NOT a byte-for-byte live capture. Endpoint
  `https://cds.unistra.fr/cgi-bin/nph-sesame/-oxp/SNV?<name>`. The parser is
  tolerant (falls back to scanning the whole doc), but if live Sesame differs
  (tag casing, attribute-based fields, nesting) the fixture + parser in
  `src/parse.rs` need a refresh. **Action**: run the ignored live test
  (`cargo test -p simbad-resolver-sesame -- --ignored`) on a networked machine to
  confirm/refresh. TAP live tests likewise (`-p simbad-resolver-tap`).
- **Sesame `common_name`** is always `None` (Sesame has no SIMBAD-`NAME` curated
  analog); enrichment via a supplied TAP resolver fills type/aliases when wanted.

## Autonomous decisions

- **Deviated from the full SpecKit implementation DAG** (tasks → checklist →
  analyze → agent-assign → verify → review → …) in favour of a direct,
  test-driven crate-by-crate build, because the goal is "take to completion,
  fully tested." `spec.md`/`plan.md`/`research.md`/`data-model.md`/`contracts/`
  were authored through SpecKit; `tasks.md` is generated for the record. Post-hoc
  quality gates (clippy/fmt/test/doc) are enforced via CI + `just lint`.
- **8-crate granular split** under `simbad-resolver-*` (see ADR-0001 / plan.md).
- **Async `Resolver`/`Cache`/`Queue` traits** (dyn-compatible via `async-trait`).
- **Settings are caller-owned config** — no persisted `resolver_settings` table.
- **Sesame standalone with optional enrichment** — no hard dep on `-tap`.
- **Durable backend = SQLite (`sqlx`)**; `redb` noted as a future pure-Rust option.
- **Dependencies unconstrained** (per maintainer): dropped astro-plan's hand-rolled
  ADQL percent-encoding, https endpoint check, `domain_core` dep, and dead `strsim`.
- **Configurable id namespace** (UUIDv5) replacing the hardcoded `astro-plan.targets`.
- **`otype_raw` escape hatch** retained alongside the closed `ObjectType`.
- **Caldwell translation lives in the facade** (so both TAP and Sesame benefit);
  direct `-tap`/`-sesame` users call `simbad-resolver-caldwell` themselves.
- **Batch `drain()` is sequential** within a pass (polite to CDS; each pending
  item processed at most once per call, transient failures released for retry).
  True bounded parallelism is a future enhancement (kept simple + correct for v1).
- **`apply_override`** makes the whole target `user-override` (sticky) and binds
  the supplied alias; returns `None` if the target id is unknown.
- **Parallel build via worktree subagents** (core/cache built directly; the 5
  leaf crates built in isolated worktrees and merged). One worktree branched
  from a pre-scaffold commit and self-corrected via `--ff-only`; no work lost.
