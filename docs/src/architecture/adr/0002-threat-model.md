# ADR 0002 — Threat model and the scope of security work

**Status:** Accepted
**Deciders:** owner, orchestrator

Read this before any security work. It exists as much to stop over-engineering
as to justify the work that remains.

## Context

FastClip stores labelled text snippets and copies them to the clipboard. The
README says "DO NOT USE FOR PASSWORDS" because the store is plaintext JSON at
`%LOCALAPPDATA%\FastClip\db`.

Two hardening options were on the table, and they are easy to conflate:

1. **Encryption at rest** — the file on disk is ciphertext.
2. **IPC boundary hardening** — plaintext never crosses into the webview; the
   frontend would call `copy_clip(id)` and Rust would write the clipboard.

## Decision

Do (1). Do not do (2).

Plaintext clip values may cross the Tauri IPC boundary and live in webview
memory. The frontend continues to receive `value` and populate edit forms with
it.

## Rationale

### Why (2) is rejected

Tauri IPC is an in-process channel between webview and Rust host. There is no
network hop and nothing on the wire to intercept. The realistic attacks on
plaintext in webview memory are:

- **script injection** — FastClip loads no remote content and renders no
  untrusted HTML. The attack surface is user-authored text.
- **devtools or a memory dump** — both require running code as the user, which
  already defeats any defence this app could mount.

The cost is immediate: the edit flow needs `value` to populate a form, so (2)
would force either a second "reveal" command reintroducing the same exposure,
or a worse editing experience on a tool whose value proposition is speed.

### Why (1) is kept

The file on disk is the durable exposure, and it leaves the machine in ways the
user does not think about: OneDrive or Dropbox profile sync, Windows File
History, backup images, a disk without BitLocker, a shared or resold machine.
Ciphertext at rest turns each of those from a disclosure into a non-event, and
unlike (2) it costs the user nothing at runtime.

## Out of scope

FastClip does not defend against an attacker who can execute code as the
logged-in user. With a key from Windows DPAPI, any process running as that user
can obtain it. The alternative is a master passphrase prompt, which is wrong for
a click-to-copy tool.

Also out of scope: clipboard contents once copied, and physical access to an
unlocked session.

## Consequences

- `backend-dev` implements an AEAD-encrypted, versioned store with a migration
  from the existing plaintext file. Losing clips on upgrade is worse than the
  plaintext being fixed, so the migration is tested first.
- **The README warning is reworded, not deleted.** Claiming FastClip is safe for
  passwords would be false: a password manager defends the "code running as the
  user" case with a passphrase that is not on the machine. Replacement text:
  *clips are encrypted at rest, but FastClip is not a password manager and does
  not protect against software running under your account.* Deleting the warning
  outright overstates what was built, and the critic blocks that.
- `println!("new_clip {:?}", clip)` in `commands.rs` is still fixed. Logging
  secrets to stdout is in scope under any threat model.
- `"csp": null` in `tauri.conf.json` is still tightened by `devops`. It keeps
  this ADR's "no script injection" premise true by policy rather than by luck.
- Contract open question 2 is closed.
