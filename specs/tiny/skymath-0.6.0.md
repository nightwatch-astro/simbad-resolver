# TinySpec: Consume skymath 0.6.0

**Branch**: `deps/skymath-0.6.0`
**Date**: 2026-07-22
**Status**: complete
**Complexity**: small

## What

Move the public coordinate return types to skymath 0.6.0 so downstream crates
use one compatible `skymath::Equatorial` type. Coordinate values and accessor
behavior stay the same, but skymath 0.5 and 0.6 types have distinct Rust
identities.

## Context

| File | Role |
|---|---|
| `Cargo.toml` | Sets the direct skymath compatibility constraint. |
| `src/types.rs` | Exposes `skymath::Equatorial` from `ResolvedIdentity::position`. |
| `src/cache/mod.rs` | Exposes `skymath::Equatorial` from `CachedTarget::position`. |
| `tests/skymath.rs` | Exercises the public typed-coordinate accessors. |
| `release-please-config.json` | Maps pre-1.0 breaking changes to minor releases. |

## Requirements

1. `Cargo.toml` accepts skymath 0.6.0 and contains no active 0.5 constraint.
2. Cargo resolves one skymath package at version 0.6.0.
3. Public `position()` accessors intentionally return the skymath 0.6 type.
4. Callers with skymath 0.5 type annotations or bounds must update to 0.6.0.
5. The release commit uses breaking Conventional Commit syntax and a
   `BREAKING CHANGE` footer.
6. Release-please interprets the migration as a 0.4.0 minor release from 0.3.5.
7. Formatting, Clippy, all-features tests, documentation, and package gates pass.

## Plan

1. Update the direct dependency with `cargo add skymath@0.6.0`.
2. Document the intentional public type migration and downstream action.
3. Mark the release commit and pull request as breaking.
4. Inspect the manifest diff, resolved dependency graph, and release policy.
5. Run the repository quality and release gates.

## Tasks

- [x] Update the direct skymath constraint with Cargo.
- [x] Verify the resolved skymath version and absence of stale constraints.
- [x] Document the public type migration and downstream dependency update.
- [x] Mark the release metadata as breaking for a 0.4.0 release.
- [x] Run formatting, Clippy, all-features tests, and documentation checks.
- [x] Run package metadata and package-build checks.

## Done When

- [x] All tasks are checked.
- [x] Tests pass.
- [x] Clippy reports no warnings.
- [x] The package builds against skymath 0.6.0.
- [x] Release metadata selects a pre-1.0 minor release.
