//! The events the backend pushes to the window.
//!
//! Contract §3. Both are emitted with `AppHandle::emit`, so every listening
//! window receives them; there is one window.
//!
//! **No emit may be silently dropped.** The pre-refactor build populated a
//! global `APP_HANDLE` inside a task spawned from `setup()`, so an early
//! `invoke` found `None` and the emit was skipped after an `eprintln!`. There is
//! no global here: the handle is a command parameter, so it exists whenever a
//! command is running, and there is no state in which it can be absent.

use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::wire::Clip;

/// The complete clip list, in display order. Never a delta.
pub const UPDATE_CLIPS: &str = "update_clips";

/// Emit `update_clips` after a mutation that has already committed.
///
/// **A failed emission does not fail the command**, and this is a decision
/// rather than an oversight. The emission happens after the store has committed,
/// so returning an error here would tell the frontend that a create which
/// succeeded had failed — leaving the user looking at an error message beside
/// the clip they had just made. The frontend's next `list_clips` recovers the
/// list; nothing recovers a mutation the user believes did not happen.
///
/// Contract §3 makes the event a guarantee, so a failure is logged at error
/// level rather than absorbed silently. It is not reachable by any modelled
/// condition: the payload is a `Vec<Clip>` of derived `Serialize` types.
pub fn emit_update_clips<R: Runtime>(app: &AppHandle<R>, clips: &[Clip]) {
    if let Err(error) = app.emit(UPDATE_CLIPS, clips) {
        // The error describes the emission, not the payload. No clip label or
        // value reaches this line.
        log::error!("the {UPDATE_CLIPS} event could not be emitted: {error}");
    }
}
