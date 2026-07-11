# Releasing

Releases are automated with [release-please] and published to crates.io with
[Trusted Publishing] (OIDC) — there is **no long-lived crates.io API token** in
CI. See [`.github/workflows/release-please.yml`](.github/workflows/release-please.yml).

## How a release happens

1. Merge conventional-commit changes (`feat:`, `fix:`, `feat!:`, …) to `main`.
2. `release-please` opens/updates a **release PR** that bumps versions and
   updates each crate's `CHANGELOG.md`. All eight crates move together
   (lockstep) via the `linked-versions` plugin.
3. The release PR is opened by the **`sjorsr-release-bot` GitHub App**, so CI
   runs on it (a PR opened with the default `GITHUB_TOKEN` would not trigger
   workflows). Review the diff and the version bump.
4. Merge the release PR. `release-please` creates the git tags and GitHub
   releases, and the `publish` job then runs `cargo publish --workspace`,
   authenticating to crates.io via OIDC.

## Required GitHub configuration

### Secrets (for the `sjorsr-release-bot` App token)

The `release-please` job mints an installation token from the
`sjorsr-release-bot` GitHub App. Set these repository secrets:

| Secret | Value |
|---|---|
| `SJORSR_RELEASE_BOT_APP_ID` | The `sjorsr-release-bot` App's App ID |
| `SJORSR_RELEASE_BOT_PRIVATE_KEY` | A PEM private key generated for that App |

The App must be installed on this repository with **Contents: read & write** and
**Pull requests: read & write** permissions.

### crates.io Trusted Publishing

crates.io does **not** support pending publishers, so each crate must be
published once manually before Trusted Publishing can be configured (see
Bootstrap below). After that, configure a trusted publisher for **each** of the
eight crates at `https://crates.io/crates/<crate>/settings` → *Trusted
Publishing* → *Add*:

- Repository owner: `srobroek`
- Repository name: `simbad-resolver`
- Workflow filename: `release-please.yml`
- Environment: *(leave blank)*

## One-time bootstrap (first publish)

Because Trusted Publishing can't be set up for a crate that has never been
published, do the first `0.1.0` publish manually with a scoped API token:

```bash
# 1. Log in with a crates.io token that has publish-new + publish-update scope.
cargo login <token>

# 2. Publish all eight crates in dependency order (cargo handles ordering
#    and waits for the index between crates).
cargo publish --workspace

# 3. Configure Trusted Publishing for each crate on crates.io (see above).

# 4. Revoke the bootstrap token — CI never needs it again.
```

From then on, every release publishes via OIDC with no stored token.

## Notes

- **Verify the first automated release PR.** Confirm it bumps every crate's
  `version` and every internal dependency requirement (`simbad-resolver-* = { …
  version = "…" }`) consistently before merging. The `cargo-workspace` plugin
  handles the dependency graph; the review is a safety check.
- **MSRV / CI.** The `ci.yml` workflow (fmt, clippy, test, doc) runs on the
  release PR via the App token, gating the release on a green build.

[release-please]: https://github.com/googleapis/release-please
[Trusted Publishing]: https://crates.io/docs/trusted-publishing
