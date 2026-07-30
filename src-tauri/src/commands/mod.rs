//! The IPC command surface.
//!
//! `docs/src/architecture/contract.md` §2 declares sixteen commands. Eleven are
//! implemented here:
//!
//! | Package | Commands |
//! | ------- | -------- |
//! | [WP-05](../../../docs/src/work/wp-05-crud.md) | `list_clips`, `create_clip`, `update_clip`, `delete_clip`, `copy_clip` |
//! | [WP-06](../../../docs/src/work/wp-06-reorder.md) | `reorder_clips` |
//! | [WP-09](../../../docs/src/work/wp-09-export-import.md) | `export_clips`, `import_clips` |
//! | [WP-14](../../../docs/src/work/wp-14-settings.md) | `get_settings`, `set_always_on_top`, `get_lock_state` |
//!
//! The remaining five — the unlock, lock and encryption commands — belong to
//! WP-07, and a command that is not implemented is not registered, so invoking
//! one rejects rather than appearing to work.
//!
//! `get_lock_state` is here rather than in WP-07 because it is step 3 of the
//! startup sequence and the sequence has no failure branch without it: a store
//! that will not open would otherwise reach the user as an error toast over an
//! empty list, which is indistinguishable from a fresh install
//! ([review 005](../../../docs/src/reviews/005-wp-05-crud.md)).
//!
//! Every command in this module carries
//! `#[tauri::command(rename_all = "snake_case")]`. That attribute is
//! load-bearing rather than decorative: Tauri's default renames argument keys to
//! `camelCase`, and contract §0 has no `camelCase` anywhere on the wire in
//! either direction. Without it `delete_clip` would expect `clipId`.

pub mod clipboard;
pub mod clips;
pub mod encryption;
pub mod events;
pub mod export_import;
pub mod lock;
pub mod settings;
pub mod wire;

/// The clipboard write both the window and the tray reach (WP-08 calls this
/// directly rather than the IPC command).
pub use clips::copy;
pub use events::UPDATE_CLIPS;
pub use export_import::{ExportResult, ImportResult};
pub use wire::{Clip, LockState};

// The command functions themselves are **not** re-exported. `#[tauri::command]`
// generates hidden items beside each function, and `generate_handler!` resolves
// them through the path it is given — so a re-export produces a name that
// compiles as a function and fails as a command. Registration uses
// `commands::clips::…`.
