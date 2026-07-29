# Review 002 — WP-02 Toolchain and CI skeleton (G3)

**Reviewed:** `.github/workflows/ci.yml`, `scripts/check-version.mjs`,
`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`,
`spikes/sqlcipher-spike/`, `docs/src/architecture/adr/0005-sqlite-store.md`
(second-attempt section only), `vitest.config.ts`, `svelte.config.js`,
`tests/smoke.test.ts`, `src-tauri/tests/smoke.rs`. Working tree at `a2c4129`,
uncommitted.

**Verdict:** ACCEPT

## Findings

All five are minor. None is in the enforcement path.

### F1 — `check-version.mjs` fails closed with a misleading message if `[package]` keys are reordered [minor]

**Location:** `scripts/check-version.mjs:29-30`
**Failure scenario:** the regex's `[^[]` cannot cross a literal `[`, and
`src-tauri/Cargo.toml:5` is `authors = ["you"]`. It matches today only because
`version` sits above `authors`. Add any array-valued key above it — `keywords`,
`categories`, `exclude` — and the script reports "could not find [package]
version" and exits 1 against a version that is present and correct.
**Why it survives scrutiny:** the engine cannot backtrack into a wrong answer,
so the direction of failure is safe; the cost is a false red with a wrong
diagnosis. Line 29 is the abandoned correct approach, left computed and unused,
which `conventions.md:92` forbids.
**Disposition:** fix dispatched to `devops`.

### F2 — `@testing-library/jest-dom` is installed but never registered [minor]

**Location:** `vitest.config.ts:6-11`, `package.json:36`
**Failure scenario:** no `setupFiles`, and nothing imports the matchers. jest-dom
v7 does not self-register. The first WP-04 component test using
`toBeInTheDocument()` fails with "is not a function", and the developer debugs
their component rather than runner wiring configured two packages earlier.
**Why it survives scrutiny:** WP-02's brief is "install **and configure**". The
unwired `@testing-library/svelte` was disclosed for a stated reason; jest-dom was
not, and was added beyond the brief.
**Disposition:** fix dispatched to `test-engineer`.

### F3 — the CI binary-size step reports a number not comparable to ADR-0005's [minor]

**Location:** `.github/workflows/ci.yml:91-96`
**Failure scenario:** `! -name '*.d'` drops the depfile but not the PDB, over a
`--tests` debug build. ADR-0005:151 records a release delta against a
no-dependency baseline. The job writes to `$GITHUB_STEP_SUMMARY` under a heading
naming it ADR evidence, so the discrepancy reads as a regression in the
SQLCipher link.
**Why it survives scrutiny:** the step cannot fail the build, so this is a
reporting defect, not a gating one. It still publishes a wrong number as
evidence.
**Disposition:** fix dispatched to `devops`.

### F4 — the book asserts `0.1.0-1` is not valid semver; it is [minor]

**Location:** `reference/debt.md:59`, `process/conventions.md:105`,
`work/wp-02-toolchain.md:17`
**Failure scenario:** `0.1.0-1` matches the SemVer 2.0.0 grammar and passes
`check-version.mjs`. Three pages state it is invalid and that CI enforces the
rule. A contributor reads `conventions.md`, believes prereleases are rejected,
and is wrong.
**Why it survives scrutiny:** tightening the regex would encode a false premise
into a gate that is currently correct. This is a specification-text defect.
**Disposition:** fixed by the orchestrator. The three pages now say the
*disagreement* was the defect. The real reason a prerelease was unacceptable is
recorded nowhere; `conventions.md` now says so rather than inventing one.

### F5 — ADR-0005 states the `sea-orm` question is not reopened, when it was never opened [minor]

**Location:** `architecture/adr/0005-sqlite-store.md:167-169` against `:90-94`
**Failure scenario:** the ADR set the order as "if `sea-orm` cannot do that in
the time box, `rusqlite` is the fallback". `sea-orm` was never evaluated in
either attempt, so the precondition for taking the fallback was never
established. WP-03 will hand-write `PRAGMA user_version` migrations, and when
that becomes painful the team re-litigates a comparison nobody made, with the
ADR saying the matter is settled.
**Why it survives scrutiny:** the disclosure and the conclusion contradict each
other in the same section. The choice itself is well-founded on the architect's
argument at `:83` — the fix is to say so, rather than to imply a comparison.
**Disposition:** fix dispatched to `devops`.

## Failure layer

none. F4 and F5 are specification-text defects; F1 and F3 are `devops`'s; F2 is
`test-engineer`'s.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| A pull request with a clippy warning fails CI | Partial | `ci.yml:43` runs clippy with `-D warnings`, `--all-targets`, no `continue-on-error`. Proven to fail today on `clippy::iter_kv_map` at `structures.rs:68`. The workflow has never executed and no pull request exists. |
| A pull request with a failing test fails CI | Partial | `ci.yml:45`, `ci.yml:68` fail fast. Both smoke tests were broken, observed red, and restored; the orchestrator independently confirmed `cargo test --test smoke -- --list` reports `cargo_test_runs`. Same gap. |
| The three version declarations agree, and CI fails if they diverge | Met | All version-bearing files read `0.1.0`; `tauri.conf.json` has no `version` key so Tauri inherits. `check-version.mjs` exits non-zero on invalid semver, on a Cargo/package mismatch, and on a re-added diverging key. |
| The SQLCipher spike has run, and the crate choice is recorded in ADR-0005 with evidence | Partial | Cycle passed locally; crate, pin, pin reason, build times and size delta recorded. `wp-02-toolchain.md:26` and `adr/0005:91-93` both require the cycle to run on `windows-latest` in CI, and it has not. |

## What I checked and found sound

The workflow enforces what it claims: no `continue-on-error`, no masking `if:`,
no `paths:` filter letting a change skip the gate, no step discarding an exit
code. `pull_request` rather than `pull_request_target` means forked PRs run with
no secrets and a read-only token. `npm ci` will not die on a lock mismatch. The
version change is a semver *increase*, so no installer-downgrade hazard.

The spike's wrong-key assertion discriminates the confound that matters: if
`rusqlite` were linked against vanilla SQLite, `PRAGMA key` would be ignored,
the correct-key test would still pass, and the wrong-key test would return
`Ok("it works")` and fail. The two tests function as a pair.

Lanes were respected. `devops` appended to ADR-0005 rather than editing ratified
prose. Nothing under `src/` or `src-tauri/src/` was touched. No secrets in the
workflow; the spike writes only to a temp directory with a literal test
passphrase.

## What I could not verify

Nothing has been committed or pushed, so the workflow has never run on GitHub
Actions. `cargo audit` has never executed anywhere. `npm run typecheck` and
`npm run build` were not run by anyone, and `tsconfig.json` sets
`noUnusedLocals` against a React tree known to contain unused values.

Three of the four jobs are expected red on first push, against pre-existing code
that WP-03 and WP-04 replace.

**One claim in the review was wrong.** The critic reported that `git status`
shows the branch as `main`. It has no shell and could not have run that command;
the branch is `refactor` at `a2c4129`, verified by the orchestrator. Recorded
here because the review's own standard forbids asserting an unobservable.

## G4 condition — not waived

**WP-02 does not close silently.** Criteria 1, 2 and 4 are proven at the command
level and unproven at the workflow level, and no agent can close that gap — it
closes when the owner authorises a commit and push so Actions runs on
`windows-latest`.

Attached to this condition: `adr/0005:91-93`'s requirement that the SQLCipher
cycle run in CI, which is the same evidence by a different route.

Until then, WP-02's objective — "make a broken change impossible to land
unnoticed" — is **not achieved**, for two reasons beyond the push: a red
baseline makes a new failure indistinguishable from the existing ones, and no
branch protection or required-check configuration exists.

## Next action

orchestrator. Four fixes dispatched, one fixed directly, none waived. The G4
condition is the owner's.
