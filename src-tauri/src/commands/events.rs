//! The events the backend pushes to the window.
//!
//! **Contract §3 declares three, and this module is all three of them.** Every
//! wire event name and every function that emits one is here, so a reader
//! asking what the backend pushes can enumerate the surface from this page
//! alone:
//!
//! | Event | Name | Emitted by |
//! | ----- | ---- | ---------- |
//! | [`UPDATE_CLIPS`] | `update_clips` | [`emit_update_clips`], from six commands |
//! | [`LOCK_STATE`] | `lock_state` | [`emit_lock_state`], from four commands |
//! | [`UNLOCK_REQUESTED`] | `unlock_requested` | [`emit_unlock_requested`], from the tray |
//!
//! All three are emitted with `AppHandle::emit`, so every listening window
//! receives them; there is one window.
//!
//! **`unlock_requested` is the one with no command behind it.** Its sole caller
//! is the tray's *Unlock FastClip* handler ([`crate::tray::on_menu_event`]), and
//! a command that emitted it would be a defect (contract §3,
//! [ADR-0013](../../../docs/src/architecture/adr/0013-unlock-requested-event.md)).
//! Its name lives here rather than beside that one caller because this module
//! exists to enumerate the category: a constant kept elsewhere would leave this
//! page answering two-thirds of the question with no sign that it was
//! incomplete.
//!
//! **No emit may be silently dropped.** The pre-refactor build populated a
//! global `APP_HANDLE` inside a task spawned from `setup()`, so an early
//! `invoke` found `None` and the emit was skipped after an `eprintln!`. There is
//! no global here: the handle is a parameter of whatever is emitting — a command
//! for the first two, Tauri's menu-event handler for the third — so it exists
//! whenever an emitter is running, and there is no state in which it can be
//! absent.

use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::wire::{Clip, LockState};

/// The complete clip list, in display order. Never a delta.
pub const UPDATE_CLIPS: &str = "update_clips";

/// The complete lock state. Never a delta (contract §3).
pub const LOCK_STATE: &str = "lock_state";

/// The user chose the tray's *Unlock FastClip* item. Payload `null` — a gesture,
/// not a fact about the store (contract §3,
/// [ADR-0013](../../../docs/src/architecture/adr/0013-unlock-requested-event.md)).
pub const UNLOCK_REQUESTED: &str = "unlock_requested";

/// Emit `lock_state`.
///
/// **The event reports the store's state, not the outcome of the call**
/// (contract, [`lock_state`]). A conversion emits at its commit point, so it
/// emits even when the command then returns an error: `enable_encryption`
/// renames the new database into place and only afterwards reopens it, and if
/// that reopen fails the store *is* converted while the command rejects with
/// `storage`. Tying this to the return value is what would let the settings view
/// claim encryption is on over a plaintext store.
///
/// A failed emission is logged and absorbed, on the same reasoning as
/// [`emit_update_clips`]: it happens after the state has already changed, so
/// returning an error here would report a conversion that succeeded as one that
/// failed.
pub fn emit_lock_state<R: Runtime>(app: &AppHandle<R>, state: &LockState) {
    if let Err(error) = app.emit(LOCK_STATE, state) {
        // The payload is four scalars — two booleans and two optional numbers —
        // so neither this line nor the payload can carry clip content or key
        // material.
        log::error!("the {LOCK_STATE} event could not be emitted: {error}");
    }
}

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

/// Emit `unlock_requested`.
///
/// **No command calls this**, and one that did would be a defect (contract §3).
/// The only caller is the tray's *Unlock FastClip* handler, which is why the
/// per-command emit table in `tests/ipc.rs` gains no row for this event.
///
/// **The payload is `null`.** The event reports a gesture, and every fact the
/// handler needs is already in the last `lock_state`
/// ([ADR-0013](../../../docs/src/architecture/adr/0013-unlock-requested-event.md)).
///
/// **A failed emission fails nothing**, and unlike [`emit_update_clips`] the
/// reason is not that state has already changed — no state changed at all. The
/// window has been raised and the prompt is on screen either way; the cost is a
/// click on the field. Nothing retries, and the caller is a native menu with no
/// error surface. Logged at error level because contract §3 makes the event a
/// guarantee.
pub fn emit_unlock_requested<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(UNLOCK_REQUESTED, ()) {
        // The payload is `null` and the error describes the emission, so
        // neither can carry clip content (ADR-0002).
        log::error!("the {UNLOCK_REQUESTED} event could not be emitted: {error}");
    }
}
