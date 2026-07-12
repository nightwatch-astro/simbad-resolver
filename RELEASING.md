# Releasing

Releases are automated with [release-please] and published to crates.io with
[Trusted Publishing] (OIDC) — there is **no long-lived crates.io API token** in
CI. See [`.github/workflows/release-please.yml`](.github/workflows/release-please.yml).

`simbad-resolver` is a single crate, so the release is a single package version
and a single `cargo publish`.

## How a release happens

1. Merge conventional-commit changes (`feat:`, `fix:`, `feat!:`, …) to `main`.
2. `release-please` opens/updates a **release PR** that bumps the version in
   `Cargo.toml` and updates `CHANGELOG.md`.
3. The release PR is opened by the **`sjorsr-release-bot` GitHub App**, so CI
   runs on it (a PR opened with the default `GITHUB_TOKEN` would not trigger
   workflows). Review the diff and the version bump.
4. Merge the release PR. `release-please` creates the git tag and GitHub
   release, and the `publish` job then runs `cargo publish`, authenticating to
   crates.io via OIDC.

## Required GitHub configuration

### Credentials (for the `sjorsr-release-bot` App token)

The `release-please` job mints an installation token from the
`sjorsr-release-bot` GitHub App. These are set on the `nightwatch-astro` org —
note the client id is a **variable**, the key is a **secret**:

| Name | Kind | Value |
|---|---|---|
| `RELEASE_APP_CLIENT_ID` | Variable | The App's Client ID |
| `RELEASE_APP_PRIVATE_KEY` | Secret | A PEM private key generated for that App |

The App must be installed on this repository with **Contents: read & write** and
**Pull requests: read & write** permissions.

### crates.io Trusted Publishing

crates.io does **not** support pending publishers, so the crate must be
published once manually before Trusted Publishing can be configured (see
Bootstrap below). After that, configure a trusted publisher at
`https://crates.io/crates/simbad-resolver/settings` → *Trusted Publishing* →
*Add*:

- Repository owner: `nightwatch-astro`
- Repository name: `simbad-resolver`
- Workflow filename: `release-please.yml`
- Environment: *(leave blank)*

## One-time bootstrap (first publish)

Because Trusted Publishing can't be set up for a crate that has never been
published, do the first `0.1.0` publish manually with a scoped API token:

```bash
cargo login <token>   # a token with publish-new + publish-update scope
cargo publish
# then configure Trusted Publishing on crates.io (see above) and revoke the token.
```

From then on, every release publishes via OIDC with no stored token.

[release-please]: https://github.com/googleapis/release-please
[Trusted Publishing]: https://crates.io/docs/trusted-publishing
