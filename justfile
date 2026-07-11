# simbad-resolver task shortcuts. Run `just` to list.
set shell := ["bash", "-cu"]

# List available recipes
default:
    @just --list

# Build the whole workspace
build:
    cargo build --workspace --all-features

# Run all tests (offline; live SIMBAD tests are #[ignore]-gated)
test:
    cargo test --workspace --all-features

# Run live SIMBAD integration tests (needs network)
test-live:
    cargo test --workspace --all-features -- --ignored

# Format check + lint (clippy denies warnings)
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Auto-format
fmt:
    cargo fmt --all

# Type-check fast
check:
    cargo check --workspace --all-features

# Docs (deny broken intra-doc links)
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps

# Remove build artifacts
clean:
    cargo clean
