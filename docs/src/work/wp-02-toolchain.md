# WP-02 — Toolchain and CI skeleton

**Objective:** make a broken change impossible to land unnoticed, before there
is any new code to break.

**Depends on:** nothing. Runs in parallel with WP-01.

## Work

### devops

Create the GitHub Actions workflow on a `windows-latest` runner. Rust checks:
`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`. Frontend
checks: typecheck, `svelte-check`, `vitest run`, build. Cache the Cargo
registry and target directory.

Fix versioning: `Cargo.toml` says `0.1.0-1`, which is not valid semver,
`package.json` has no version, and neither agrees with `tauri.conf.json`. Pick
one source of truth and enforce agreement in CI.

Add `cargo audit` and `npm audit`.

Prove each check can fail before declaring it works.

### test-engineer

Install and configure the runners so the workflow has something to call:
Vitest with `@testing-library/svelte` and the Tauri mock API, and a `cargo test`
target. One trivial passing test per side, purely to prove the wiring.

### Others

No work.

## Definition of done

- A pull request with a clippy warning fails CI.
- A pull request with a failing test fails CI.
- The three version declarations agree, and CI fails if they diverge.

## Risks

Rust CI without caching is slow enough that people stop waiting for it, which
turns the gate into decoration.
