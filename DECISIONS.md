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
- **Sesame endpoint/format**: using the CDS Sesame `sim-nph` XML output; parser
  is defensive (may need a fixture refresh if CDS changes the format).

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
