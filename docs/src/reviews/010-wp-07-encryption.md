# Review 010 — WP-07 Optional PIN-gated encryption (G3)

**Reviewed:** `src-tauri/src/commands/{lock,encryption,events,wire}.rs`,
`src-tauri/src/crypto/`, `src-tauri/src/storage/{store,connection,mod}.rs`,
`src-tauri/src/tray.rs`; `src/lib/state/lockState.svelte.ts`,
`src/lib/components/{PinInput,PinPrompt,SettingsPanel}.svelte`;
`src-tauri/tests/{ipc,two_factor,keyfile,kill_mid_conversion}.rs`,
`tests/{pin-input,pin-prompt,settings-panel-encryption,app-lock-transition}.test.ts`.

**Verdict:** ACCEPT on the second pass — 1 minor finding in the tests layer,
which does not gate the package.

This is the package carrying the `BLOCK` risk. A false ACCEPT costs more here
than anywhere else in the refactor, so the second pass was scoped to the fix and
to the two-factor property rather than re-reading everything.

## First pass — F1, and why it was raised

`lock` could fail **after** the store had already locked. The emission of
`lock_state` sat behind a `?` on a step that ran after `Store::lock` had taken
effect, so a failure in that step returned an error to a frontend still showing
the unlocked state, over a store that was locked. The user sees an unlocked
interface backed by a locked store: every subsequent action fails for a reason
the interface contradicts.

## The fix, and what the second pass established about it

`commands/lock.rs:250-259` now contains three statements, ordered so that the
only fallible one runs first:

```rust
store.lock()?;                                     // the only step that may refuse
tray::refresh(&app);                               // absorbs its own failures
events::emit_lock_state(&app, &LockState::LOCKED); // reads nothing, cannot fail
```

The critic did not accept "returns `()`" as sufficient and checked one level
down. `tray::refresh` and `emit_lock_state` match and log every fallible call
inside them; `lock_recovering` (`storage/mod.rs:63`) **recovers** a poisoned
mutex rather than unwrapping it, so a panic elsewhere cannot turn into a panic
here. Inside `Store::lock` the only steps after `held.take()` are
`checkpoint_and_close` — which matches and logs both the checkpoint and the
close — and two field assignments.

So the rule *nothing after the lock takes effect may fail* holds for the whole
span, not just the visible three lines.

**The negative control is real.** In
`a_lock_still_emits_when_the_key_material_has_become_unreadable`
(`tests/ipc.rs:2033`) the preceding `enable_encryption` has already pushed the
unlocked payload into the listener. Restoring the `?` therefore leaves
`states.last()` holding the *unlocked* payload and the assertion fails with the
exact frontend-state-after-lock shape the finding described. A test that fails
for the right reason, not a tautology.

**The `unlock` path was closed too**, without being asked. Every step after
`store.unlock_with(dek)?` is absorbed, so neither command can return an error
after the store has changed state. The wire payload is unchanged, so the
frontend was untouched.

## Findings

### F1 — no test fails if the conversion guard is deleted from the commands [minor]

**Location:** `src-tauri/src/storage/store.rs:691-729`, against
`src-tauri/src/commands/encryption.rs:69` and `:140`
**Failure scenario:** delete `let _conversion = store.begin_conversion();` from
both conversion commands and the suite still passes. The only test referencing
the guard calls `begin_conversion` **directly** from six spawned threads, so it
asserts that a `std::sync::Mutex` excludes — a property of the standard library
— not that either command takes it.
**Why it survives scrutiny:** no other test invokes a conversion command from
more than one thread, and the guard has no observable effect single-threaded.
The counter-argument that the race is unreachable today argues for the severity
being minor, not for the gap being acceptable: the guard's entire justification
(`store.rs:276-277`) is protecting a change that has not happened yet, and it is
invisible to the suite during exactly that change.
**Disposition:** routed to `test-engineer` as a follow-up, not as a gate.
**Closed 2026-07-30**, by the route the critic proposed and could not compile.

`WebviewWindow<MockRuntime>` **is** `Send + Sync` — the bound the critic named as
the part that might refuse. A shared `&MockWindow` moved into two
`std::thread::scope` closures type-checks with no `unsafe`, so
`two_concurrent_enables_never_leave_an_encrypted_store_with_no_key`
(`tests/ipc.rs:2528-2593`) drives two real `enable_encryption` calls through the
real invoke path, released together by a `Barrier`.

**No sleep creates the race window.** It comes from the command's own cost:
`write_and_prove_key_material` runs the shipped Argon2id tuple twice, so driving
the real command rather than a cheapened fixture is what makes the overlap wide.

**The first version of the test was not sufficient, and the agent found that
itself.** Asserting only that `keyfile` exists passed a run in which both racing
calls returned `Ok(Null)` while `keyfile` held a key that opened nothing. The
assertion is now that at least one of the two PINs unlocks a **fresh application
built over the same directory** — a postcondition about recoverability rather
than about a file existing.

**Red, with the guards commented out at both call sites:**

```
[222222] end after 17.2156665s: Ok(Null)
[111111] end after 17.2867991s: Ok(Null)
panicked at tests\ipc.rs:2591:
the store is encrypted and neither PIN used to enable it unlocks it any more.
Outcomes were [Ok(Null), Ok(Null)]
```

Both calls believed they had succeeded. Neither PIN could recover the store.

**The caveat, stated rather than smoothed over.** The red is probabilistic — 2 of
roughly 9 unguarded runs failed; others produced a benign double-abort with no
encrypted result, which the test correctly does not flag. The assertion is
deterministic given the state on disk; the *frequency* with which unguarded code
reaches the bad state is not 100%. Forcing determinism would need a test-only
synchronisation hook inside `enable_encryption`, which is production code and was
correctly left alone.

**Cost:** each invocation pays the real Argon2id price twice, so `tests/ipc.rs`
now runs 93-122 seconds. Deliberate — a cheaper KDF would narrow the window the
test exists to catch.

**Not covered, and named:** `disable_encryption`'s guard has no symmetric test,
and no enable-versus-disable race was attempted. It shares the same guard and the
same `store.convert` path, so the gap is real.

The stale `two_factor.rs` header was rewritten to name the duplication and its
risk. `attempt_unlock` remains a reassembly: routing it through the real `unlock`
command would drag the `tests/ipc.rs` mock-application machinery into a file that
is currently a pure crypto property test, which would trade the file's
directness for tidiness. That trade was declined, correctly.

## Acceptance criteria

| Criterion | Met | Evidence |
|---|---|---|
| The emission is unfailable on the `lock` path | Yes | `lock.rs:250-259`; both post-lock calls return `()`; `checkpoint_and_close` absorbs at `connection.rs:161-170` |
| The emission is unfailable on the `unlock` path | Yes | `lock.rs:132-151`; every step after `unlock_with` absorbed, including the snapshot failure |
| `lock`'s payload is the contract-exact constant | Yes | `wire.rs:130-135` against `contract.md:1051`; asserted at the wire |
| `begin_conversion` precedes the first destructive write | Yes | `encryption.rs:69` before `:86`; `:140` before `:161`; the second caller returns `wrong_state` without reaching `discard_key_material` |
| The guard's placement reasoning is correct | Yes | A re-check under the connection mutex is genuinely too late — B's clobber and B's delete both precede A's commit |
| The serialisation argument is sound | Yes, on the resolved versions | See below |
| The fix opened nothing | Yes | Three emission sites traced; key-material writers mutually exclusive by state; no lock-order inversion |
| The guard's presence in the commands is covered by a test | **No** | F1 |

## The citation that did not survive

`backend-dev` justified the design partly on how Tauri serialises command
dispatch, citing `tauri-macros 2.0.4` and `wry 0.48.1`. **This build links
`tauri-macros 2.6.3` and `wry 0.55.1`** (`Cargo.lock:4228-4229`, `:5861-5862`).

The critic re-checked the claim against the versions that actually compile and
found it holds verbatim — `ExecutionContext::Blocking` is the parse default,
`Async` is set only by `asyncness.is_some()` or the attribute, `body_blocking`
emits an inline call, and `CoInitializeEx(..., COINIT_APARTMENTTHREADED)` is the
first statement of webview creation with the IPC handler hung off that
controller. No command in the crate is `async`.

**The conclusion survives; the evidence trail as written does not.** A future
reader following those paths would be reading crates this build does not link.
Recorded here because a correct conclusion resting on a citation to the wrong
version is a trap laid for whoever checks it next.

## What the critic verified beyond the fix

**The two-factor property**, because it carries the `BLOCK`. `keyfile::read`
performs the DPAPI unprotect before anything else, and every caller then derives
Argon2id over the PIN and unwraps through the AEAD. There is no path from one
factor to the DEK. `tests/two_factor.rs` has a real control
(`both_factors_together_open_the_store`) alongside both single-factor attackers,
and `a_key_derived_from_the_pin_is_not_the_key_the_store_is_encrypted_with`
closes the PIN-as-entropy design failure directly.

Also checked: `Store::lock` with `held == None` — reachable after
`CommittedButClosed` — still clears the flag, zeroises, and emits the correct
payload without panicking. `discard_intermediate` touches only `keyfile.new`, so
the sweep is not a second deletion path. `disable_encryption`'s abort does not
delete `keyfile`, so the destructive shape is `enable`-only, matching the
guard's own table. `change_pin` lacks a guard, and that is defensible rather
than an omission: it requires the opposite `is_locked()` answer to `unlock` and
the opposite `encryption_enabled()` answer to `enable_encryption`, and it never
deletes key material.

## What could not be verified

The critic has no shell. The 395 Rust tests, 201 frontend tests, `fmt`,
`clippy`, `typecheck`, `svelte-check` and `build` are the orchestrator's
observations — it read whether assertions *can* fail, not whether they *did*
pass.

Its endorsement of the serialisation argument covers what two source reads show.
It did **not** trace `tauri 2.11.5`'s custom-protocol handler to confirm the
`ipc://` path also dispatches on that thread. That unproven gap is why
`begin_conversion` earns its place, and why the serialisation claim is treated
as an explanation rather than as load-bearing. Depending on it instead of the
guard would be the mistake.

Real Windows runtime behaviour is also unobservable from a static read: that
`tray::refresh` does not deadlock when called from the thread it dispatches to
rests on `tauri-runtime-wry`'s inline main-thread path, inferred rather than
read, and on every mutating command having shipped this pattern since WP-05.

## Two things noted and not raised

`storage.md` describes "the mutex" singular while the implementation now has
three — connection, status, conversion — with the ordering rule living only in a
Rust doc comment at `store.rs:18-21`. The status mutex predates this round and
was accepted, so relitigating it in a pass scoped to the fix would be out of
bounds. It is worth an architect sentence whenever `storage.md` is next opened.

`tests/two_factor.rs` carries a header saying `unlock` does not exist yet, and
its `attempt_unlock` is a reassembly of the sequence rather than the shipped
command. The steps still match, so the tests are not wrong — the header is.
Routed to `test-engineer` with F1.
