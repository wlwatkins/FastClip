# WP-12 — Release pipeline

**Objective:** someone who was not here can produce a signed installer from a
clean checkout.

**Depends on:** every other package.

## Work

### devops

Produce the NSIS installer through `tauri build` and attach it to a tagged
GitHub release. Investigate `tauri-action`.

Address code signing. An unsigned Windows installer means every user meets a
SmartScreen warning. If signing requires a certificate the owner does not have,
document that plainly rather than leaving it unmentioned.

Tighten `"csp": null` in `tauri.conf.json`. ADR-0002's reasoning depends on the
webview not executing injected script; that premise should hold by policy
rather than by luck.

Review `src-tauri/capabilities/` for the narrowest permission set that works.

### test-engineer

Install the built artefact on a clean machine or VM and confirm first-run
behaviour, including the migration path from a pre-refactor install.

### critic

Check that no secret, certificate or key is committed. Check that the shipped
capability set is not broader than the app uses.

## Definition of done

- A tag produces a downloadable installer.
- CSP is set.
- First run on a clean machine works, including migration.
- Signing is either done or documented as blocked, with what the owner must
  provide.

## Risks

The migration has only ever run in tests until this point. First-run on a real
pre-refactor install is the first honest test of WP-07.
