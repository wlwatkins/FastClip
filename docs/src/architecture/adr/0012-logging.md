# ADR 0012 — A log sink exists, writes to a file, and never contains clip content

**Status:** Accepted.
**Deciders:** architect

## Context

`Cargo.toml` declares the `log` facade and nothing installs an implementation.
There are 28 `log::` call sites and every one of them is discarded at runtime.
[`conventions.md`](../../process/conventions.md) requires "a logging facility
with levels" and names no owner, and this has now been raised independently by
two developers and the critic.

This is not a tidiness problem, because **several decisions in this book are
load-bearing on the log existing**. Each of them absorbs a failure on the
explicit ground that it is recorded rather than lost:

| Decision | What it absorbs |
| -------- | --------------- |
| [Contract §3](../contract.md#update_clips) | a failed `update_clips` emission after a committed mutation |
| [Storage — post-commit](../storage.md#failures-after-the-commit-point-are-absorbed-not-reported) | a failed `keyfile` or sidecar delete after a conversion has committed |
| [Storage — deletes](../storage.md#when-a-delete-fails) | the startup sweep and the stray-`keyfile` delete |
| [Storage — locking](../storage.md#locking-on-demand) | a failed checkpoint or close during `lock` |

Without a sink those all read "absorbed silently", which is the outcome each was
written to avoid. One further case is worse than silent: `colour.rs` holds the
**only** statement anywhere of the remedy for a pre-WP-10 development store —
delete `~/.fast-clip/` and restart — and discards it before it is written
anywhere a developer could read it.

## Decision

**Install `tauri-plugin-log`. Targets: stdout and a rotating file. The webview
target is off. No clip content is ever logged, at any level.**

| | |
| --- | --- |
| Crate | `tauri-plugin-log`, the Tauri-maintained plugin, version resolved and pinned by `devops` at the moment of installation |
| Targets | `Stdout` and `LogDir` (a rotating file beside the store) |
| Webview target | **Off** |
| Default level | `Info` in release, `Debug` in development |
| Forbidden content | any clip `label` or `value`, any `pin`, the DEK, the salt, any wrapped blob, any export or import file content |

### Why a file and not stdout alone

A released FastClip is a windowed Windows application with no console attached,
so a stdout-only sink is a sink that exists during development and not when a
user hits the failure it was written for. The four absorbed failures above are
precisely the ones a user would report as "it stopped updating" or "it says
encryption is off and I turned it on" — diagnosable from a file and from nothing
else.

`env_logger`, `fern` and `tracing-subscriber` were considered. Any of them would
work; `tauri-plugin-log` is chosen because it is maintained against Tauri 2,
handles rotation and target selection without hand-rolled setup, and is the
answer a reader of a Tauri project expects to find. This is not a decision worth
spending novelty on.

### Why the webview target is off

It would send backend log lines into the frontend console, which is a second
place clip content could reach and a channel across the seam that
[the contract](../contract.md) does not describe. The IPC boundary is the only
described channel and a log target is not going to become the second.

### The content rule is the point of this ADR

[Criterion 6](../../product/spec.md#8-acceptance-criteria) is *"No clip `value`
appears in any log, in stdout, or in any file other than the encrypted store and
a user-initiated export."* Installing a file sink creates a new file in
`~/.fast-clip/` and therefore a new way to breach that criterion — one that no
amount of care in the storage layer prevents.

So the rule is stated as an obligation rather than as guidance: **a `log::` call
that formats a clip `label` or `value` is a `BLOCK`-level finding**, and it is
worth noting that the existing call sites already comply — `events.rs` logs the
emission error and not the payload, and `colour.rs` logs the token name, which is
a palette token rather than user content.

`log::error!("{error}")` on a `ClipError` is safe by construction: no variant
carries a `label` or a `value`
([contract §4](../contract.md#4-errors)). `not_found` carries a `clip_id` and
`invalid_input` carries a field *name*, neither of which is content. That is a
property of the error type worth keeping, and a variant that ever carried clip
text would break it.

## Consequences

- **A new file in `~/.fast-clip/`**, recorded in
  [the storage Files table](../storage.md#files) with the rest, so nothing that
  copies, clears or inspects the directory is surprised by it.
- **The log is not encrypted, and is readable while the store is locked.** That
  is safe only because of the content rule, which is the whole reason the rule is
  an obligation rather than a preference.
- **`test-engineer` gains an assertion that is cheap and worth having**: run a
  session that creates, copies, edits and deletes a clip with a distinctive
  `value`, then grep the log file and stdout for it. Criterion 6 is otherwise
  tested only by inspection.
- **Four absorbed-failure decisions become true as written.** They currently
  claim a log line that does not happen.
- **The pre-WP-10 colour remedy becomes reachable**, which is the one case where
  the log is the only channel the message has.
- **`devops` owns the version pin**, as with every other dependency
  ([ADR-0005](./0005-sqlite-store.md) set that precedent by deferring a crate
  choice to a spike rather than naming a version from memory).

## Revisit if

A support workflow ever wants a user to send a log file. That is a different
artefact with different content rules — it would need a redaction pass and an
explicit user action — and it is a product decision before it is an
architectural one.
