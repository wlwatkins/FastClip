# Inherited debt

Found in the pre-refactor codebase during the initial read. This is the
baseline the refactor is measured against, and a calibration set for the
`critic`: every item is a real defect a human shipped without noticing.

The list is incomplete. It came from one pass over roughly 600 lines.

## Correctness

| Defect | Location |
| ------ | -------- |
| `APP_HANDLE` is populated inside `tauri::async_runtime::spawn` within `setup()`. An `invoke` arriving first hits `None`, and the failure path is `eprintln!("App handle not lockable")`, so the emit is skipped and the frontend never updates. Fix by setting it synchronously, or by using Tauri's managed `State` instead of a `lazy_static` global. | `src-tauri/src/lib.rs` |
| `to_vec()` iterates a `HashMap`, so clip order changes between calls and buttons move under the cursor. Now also a specification violation. | `src-tauri/src/structures.rs` |
| `save()` writes over the live database with `fs::write`. A crash mid-write truncates it and the user loses every clip. | `src-tauri/src/structures.rs` |
| `new_clip` and `update_clip` both call `insert_or_update_clip`, so updating a nonexistent id creates one instead of erroring. | `src-tauri/src/commands.rs` |
| `DataBase::new()` uses `expect()` on the config directory, directory creation, save and load. Any of them takes the app down with no message. | `src-tauri/src/structures.rs` |

## Security

| Defect | Location |
| ------ | -------- |
| `println!("new_clip {:?}", clip)` and its siblings print the clip's `value` to stdout. In scope under any threat model. | `src-tauri/src/commands.rs` |
| The store is plaintext JSON. | `%LOCALAPPDATA%\FastClip\db` |
| `"csp": null` disables Content Security Policy. [ADR-0002](../architecture/adr/0002-threat-model.md) relies on the webview not executing injected script; that premise should hold by policy. | `src-tauri/tauri.conf.json` |

## Dead weight

| Defect | Location |
| ------ | -------- |
| `surrealdb 2.2.0` is declared and imported nowhere — a database engine compiled into every build for nothing. It also appears in the README's to-do list, which is presumably how it arrived. | `src-tauri/Cargo.toml` |
| A `<canvas>` measures text and computes `_truncatedLabel` inside a `ResizeObserver`, then the component renders `fast_clip.label` raw. The mechanism is dead and re-runs on every resize. | `src/Components/Clip.tsx` |
| `_copied` is set and cleared on a timer, read by nothing. The animation that would use it is commented out. | `src/Components/Clip.tsx` |
| Empty file. | `src/cssVariableResolver.ts` |
| Mantine and Tailwind are both installed and both used. | `package.json` |

## Type safety

| Defect | Location |
| ------ | -------- |
| `SetClips(event.payload as Array<FastClip>)` — the payload is raw JSON, those objects are not `FastClip` instances, and nothing validates their shape. | `src/Components/Viewport.tsx` |
| `FastClip`'s constructor mints a fresh UUID, so anything built through it carries an id the backend never agreed to. Two sources of truth for identity. | `src/Classes/FastClip.tsx` |
| `key={index}` on a list whose every member carries a UUID. | `src/Components/Viewport.tsx` |

## Process

- Zero tests. No test runner in either ecosystem.
- No CI. Nothing prevents a broken commit landing.
- `version = "0.1.0-1"` is not valid semver, `package.json` has no version, and
  neither agrees with `tauri.conf.json`.
- Three inert fields (`icon`, `visible`, `clear_time`) serialised to disk and
  read by nothing. Removed by [spec §3](../product/spec.md).
