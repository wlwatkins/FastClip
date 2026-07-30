# Recurring defect classes

Four failure shapes have each appeared more than once in this refactor. They are
recorded here because a defect that recurs is a process fault, not a coding
mistake, and the fix belongs upstream of the code.

Each entry names the shape, lists the instances with links, and states the check
that would have caught it.

---

## 1. Two spellings of one fact, agreeing today, with nothing making them agree

The most common by a wide margin. Two places state the same thing. They match at
the moment they are written, so every test passes and every review sees a
correct system. Nothing forces them to keep matching.

| # | The fact | The two spellings | Found by |
| - | -------- | ----------------- | -------- |
| 1 | Which commands are registered | `generate_handler!` in `lib.rs` and a copy in `tests/ipc.rs` | `test-engineer`, which declined to unify it and recorded the residual risk; `backend-dev` and `critic` then agreed independently it needed its own package |
| 2 | When the tray must be rebuilt | The call to `tray::refresh` repeated at each mutation site | [Review 007](../reviews/007-wp-06-reorder.md) |
| 3 | What `LockState` is for a given store state | Derived in more than one place | [Review 006](../reviews/006-wp-14-settings.md) |
| 4 | The colour field's label | `CLIP_COLOUR_FIELD_LABEL` in the deck, and `legend = "Colour"` inline in `ColourPicker.svelte:8` | [Review 011](../reviews/011-wp-11-copy-deck.md) F3 |
| 5 | The string "Unlock FastClip" | `copy.md:78`, `tray.rs:83`, and a third copy at `PinPrompt.svelte:98` | [Review 011](../reviews/011-wp-11-copy-deck.md) F3 |
| 6 | Whether `unlock` exists | `two_factor.rs`'s header says it does not; the command shipped in WP-07 | [Review 010](../reviews/010-wp-07-encryption.md) |
| 7 | How many mutexes the store has, and in what order they are taken | `storage.md` said "the mutex"; the code has three, with the ordering rule only in a doc comment at `store.rs:18-21` | [Review 010](../reviews/010-wp-07-encryption.md). **Closed** — see below |
| 8 | How many commands exist, and how many places call `tray::refresh` | `commands/mod.rs:3-15` says eleven of sixteen are implemented; `command_handler!` registers sixteen. `tests/ipc.rs:1344` and `:1482` say "six call sites today"; there are nine | [Review 012](../reviews/012-wp-08-tray.md), noted out of lane |

**Why it keeps happening.** Duplication is invisible to every tool in this
project. The compiler is satisfied, `clippy` is satisfied, and the tests pass
because both copies are right. Only a reader holding both in mind at once can
see it, which is why the critic finds these and nothing else does.

**The check.** When a fact appears in a second place, one of **four** things must
be true before it lands:

| Remedy | Used by |
| ------ | ------- |
| The second spelling is derived from the first | Instance 1, closed with a `command_handler!` macro |
| A test fails if they diverge | The rebuild-trigger set, where `CASES.len() == 16` forces a row for any new command — [review 012](../reviews/012-wp-08-tray.md) calls this out as the pattern to copy |
| **The second spelling is deleted and replaced by a pointer** | Instance 7, closed 2026-07-30 |
| The duplication is recorded as accepted, with the reason, so the next reader inherits the knowledge instead of the surprise | The fallback, not the goal |

**Prefer deletion over agreement.** Instance 7 is the worked example.
`storage.md` now carries the acquisition order in one section, and the Rust doc
comments that used to restate it name that section instead. Two spellings became
one spelling and a pointer, so there is nothing left to drift. The architect
reached for this remedy rather than the three listed above and was right to:
deriving or testing a *prose* invariant is not available, and accepting the
duplication would have preserved the fault.

The single-home rule applies to the **rule**, not to the reasoning. The race
analysis justifying `Store::begin_conversion` stays beside that function; the
page links to it rather than copying it.

---

## 2. A test that passes whether or not the behaviour exists

A test that cannot fail is worse than no test, because it reports coverage that
does not exist and it survives the change it was written to catch.

| # | The test | Why it could not fail | Found by |
| - | -------- | --------------------- | -------- |
| 1 | Lock clears the search query | Both assertions were satisfied by the component unmounting, so deleting `closeSearch()` left it green | [Review 009](../reviews/009-wp-13-search.md) F1 |
| 2 | Only one conversion may hold the guard | Calls `begin_conversion` directly from six threads, so it asserts `std::sync::Mutex` excludes — not that either command takes the guard | [Review 010](../reviews/010-wp-07-encryption.md) F1 |
| 3 | A reorder guard under an active query | The query filtered the list to one clip, so `moveClip`'s bounds check no-opped the move regardless of the guards under test | `test-engineer`, probing its own draft |
| 4 | `reorderClips` not called | The mock was never wired into the harness, so `not.toHaveBeenCalled()` asserted against a function nothing could call | `test-engineer`, probing its own draft |
| 5 | A **red proof** that no toast appears on `locked` | `waitFor(() => expect(toastText()).toBe(""))` passed *even with the bug reintroduced* — `waitFor` succeeds on its first poll, before the rejected promise's `.catch` runs. Replaced with an awaited delay then a direct `expect` | `test-engineer`, checking its own red proof |
| 6 | The `import.field` surrogate-pair test | 200 repetitions of one 2-unit character put the cut on a character boundary under **either** counting axis, so it could only ever catch a length miscount, never a real mid-surrogate split | `test-engineer`, re-deriving what the test could detect |
| 7 | The conversion-guard race test, first draft | Asserting `keyfile` exists passed a run where both racing calls returned `Ok(Null)` and `keyfile` held a key that opened nothing | `test-engineer`, on its own draft |

**Why it keeps happening.** Writing a passing test feels like finishing. The
question that separates a real test from a decorative one is not *does it pass*
but *what would make it fail*, and that question is only asked deliberately.

**The check.** Red before green, applied to the specific assertion rather than
the file. Delete the behaviour, watch the test fail, restore it. Five of the
seven were caught this way by the agent that wrote them, which is the cheapest
place to catch them.

**Red-proof the red proof.** Instance 5 is the one worth studying: the *test of
the test* was itself vacuous. An asynchronous absence — "no toast appears" — is
the hardest thing in this class to assert, because the absence is also true
before the code that would produce it has run. `waitFor` resolving on its first
poll is not evidence; give the wrong behaviour a turn to happen, then assert.

**Ask what the test can detect, not what it is named.** Instances 6 and 7 both
passed, both tested the right function, and both were named for a property they
could not have caught. Neither red-before-green nor a passing run would have
exposed them — only re-deriving, from the input, which wrong implementations
survive it.

**The near miss worth remembering.** Instance 1's test was written to survive a
refactor, and it did. **Surviving a refactor and detecting a regression are
different properties**, and it had only the first. "Would this still pass after
the change I expect" is a weaker question than "would this fail if the behaviour
were removed".

---

## 3. Recorded as fixed without the fix being on disk

Both instances are the orchestrator's.

| # | What was claimed | What was true | Cost |
| - | ---------------- | ------------- | ---- |
| 1 | Production code intact after an agent crashed mid red-before-green | `requestDelete` at `App.svelte:67` still called `deleteClip` directly instead of opening the confirmation | Caught only because the agent had been told to verify rather than trust the orchestrator |
| 2 | [Review 008](../reviews/008-wp-09-export-import.md) F2 fixed — a version below 1 renders a false sentence | The architect ratified the contract rule; `export_file.rs:214` was never changed. Contract and code disagreed for two days while the review recorded the finding closed | Found by chance during an unrelated spot-check |

**Why it keeps happening.** In instance 1 the orchestrator read the neighbouring
function and generalised. In instance 2 it read the architect's report and
treated an intention as an outcome.

**The check.** *"I checked" has to mean the specific thing, not the
neighbourhood.* A finding is closed when the change is on disk, not when an
agent says it will be. An agent's report is evidence about what the agent did,
not about the state of the tree.

---

## 4. A correct conclusion resting on a citation that does not match the build

| # | The claim | The citation | The reality |
| - | --------- | ------------ | ----------- |
| 1 | Tauri serialises command dispatch, so a re-entrant conversion cannot interleave | `tauri-macros 2.0.4`, `wry 0.48.1` | This build links `tauri-macros 2.6.3` and `wry 0.55.1`. The critic re-checked and the claim holds verbatim on the resolved versions — but a reader following the cited paths reads crates this build does not link |
| 2 | `hex` and `SALT_LEN` are unused | Stale `rust-analyzer` diagnostics forwarded by the orchestrator | `backend-dev` verified both claims were wrong and reported so rather than making a no-op change |

**Why it matters even when the conclusion is right.** A citation is a promise
that the next reader can re-derive the claim. A wrong one converts a checkable
argument into an unfalsifiable one, and it fails at exactly the moment someone
is trying to decide whether the code is still correct.

**The check.** Cite the resolved version from `Cargo.lock`, not the one in
`Cargo.toml` and not the one you remember. Re-verify a stale diagnostic against
a fresh build before acting on it — six such diagnostics in this project
reported compile errors that were not real, and one that was.

---

## What these have in common

Three of the four are invisible to the toolchain. `cargo test`, `clippy`,
`tsc`, `svelte-check` and `vitest` were all green while every instance above was
present. That is not a gap in the tools; it is the boundary of what a tool can
check.

The consequence for how this team is run: **the review gate is not a formality
to be compressed when a change is small.** Every instance in this page was found
by a reader holding two things in mind at once, and none of them would have been
found by running the suite again.
