//! The IPC command surface.
//!
//! `docs/src/architecture/contract.md` §2 declares sixteen commands. Five are
//! implemented here, and they are the five [WP-05](../../../docs/src/work/wp-05-crud.md)
//! owns: `list_clips`, `create_clip`, `update_clip`, `delete_clip` and
//! `copy_clip`. The remaining eleven belong to later packages — reorder to
//! WP-06, the lock and encryption commands to WP-07, export and import to
//! WP-09, the settings pair to WP-14 — and a command that is not implemented is
//! not registered, so invoking one rejects rather than appearing to work.
//!
//! Every command in this module carries
//! `#[tauri::command(rename_all = "snake_case")]`. That attribute is
//! load-bearing rather than decorative: Tauri's default renames argument keys to
//! `camelCase`, and contract §0 has no `camelCase` anywhere on the wire in
//! either direction. Without it `delete_clip` would expect `clipId`.

pub mod clipboard;
pub mod clips;
pub mod events;
pub mod wire;

/// The clipboard write both the window and the tray reach (WP-08 calls this
/// directly rather than the IPC command).
pub use clips::copy;
pub use events::UPDATE_CLIPS;
pub use wire::Clip;

// The command functions themselves are **not** re-exported. `#[tauri::command]`
// generates hidden items beside each function, and `generate_handler!` resolves
// them through the path it is given — so a re-export produces a name that
// compiles as a function and fails as a command. Registration uses
// `commands::clips::…`.
