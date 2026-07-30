//! The main window, and the one window property FastClip owns.
//!
//! **Always-on-top is applied by the backend, in two places**: here at window
//! creation from the persisted value, and again by
//! [`crate::commands::settings::set_always_on_top`] when the user toggles it
//! (contract, `set_always_on_top`). The frontend calls no Tauri window API for
//! it and its capability set does not grant `core:window:set_always_on_top`.
//!
//! Two writers of one window property is two sources of truth, and the startup
//! application has to be backend-side regardless: the window exists before the
//! webview does, so a flag applied from the frontend would show the window in
//! the wrong state and then correct itself.
//!
//! **That is why `tauri.conf.json` carries `"create": false` on the main
//! window.** Tauri builds a config window during its own setup with exactly the
//! properties written in the file, and `alwaysOnTop` there is a constant — there
//! is no point at which a persisted value could reach it. With `create: false`
//! FastClip builds the same window from the same config and overrides that one
//! property before the window exists. Do not restore the default and "fix it
//! afterwards" with a `set_always_on_top` call: that is the visible flash this
//! arrangement removes.

use tauri::{Manager, Runtime, WebviewWindowBuilder};

use crate::error::ClipError;

/// The label the capability set and `tauri.conf.json` both name.
pub const MAIN_WINDOW_LABEL: &str = "main";

/// Build the main window from its `tauri.conf.json` entry, with the persisted
/// always-on-top value applied at creation.
///
/// Returns whether the window exists. **The caller must act on `false` by
/// stopping**, and [`crate::run`] does.
///
/// **An earlier version of this returned `()`**, on the reasoning that "a
/// backend that gives up leaves nothing to show the user". That reasoning is
/// right for [startup recovery](crate::storage::Store::open) and **inverts
/// here**: the thing that failed *is* the surface for showing the user, so
/// continuing is what leaves nothing. The comment is corrected rather than
/// deleted, because the reasoning in it is what produced the defect and the next
/// reader will otherwise re-derive it.
///
/// What continuing cost, concretely: Tauri raises `ExitRequested` only when the
/// last window is *destroyed*, and a window that was never created cannot be —
/// so `app.run` never returns and the process sits in its event loop with no
/// interface. `main.rs` sets `windows_subsystem = "windows"`, so there is no
/// console in a release build, and the tray ([WP-08](../../../docs/src/work/wp-08-tray.md))
/// is a copy menu rather than somewhere an error can be read. The user's only
/// remedy was Task Manager, and every retry added another invisible process.
///
/// **Not a `Result<(), ClipError>`.** Nothing branches on which of the three
/// failures happened — the caller stops in all three cases — and each already
/// logs the one fact that distinguishes them. A typed error whose only consumer
/// discards the type is a type nobody reads.
#[must_use = "a false return means there is no window, and the caller must stop"]
pub fn create_main<R: Runtime, M: Manager<R>>(manager: &M, always_on_top: bool) -> bool {
    let config = match manager
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN_WINDOW_LABEL)
    {
        Some(config) => config.clone(),
        None => {
            log::error!("tauri.conf.json declares no window labelled {MAIN_WINDOW_LABEL}");
            return false;
        }
    };

    let builder = match WebviewWindowBuilder::from_config(manager, &config) {
        Ok(builder) => builder,
        Err(error) => {
            log::error!("the main window configuration could not be read: {error}");
            return false;
        }
    };

    // After `from_config`, so it overrides the constant in the file rather than
    // being overridden by it.
    //
    // The realistic cause of a failure here is WebView2 being absent or broken.
    if let Err(error) = builder.always_on_top(always_on_top).build() {
        log::error!("the main window could not be created: {error}");
        return false;
    }

    true
}

/// Apply always-on-top to every open window.
///
/// There is one window. Every open window is used rather than the label, so a
/// second one added later cannot silently miss the setting.
///
/// **`internal` is the honest variant for a failure here.** `set_always_on_top`
/// declares `invalid_input` and `storage`, and neither describes a refused
/// window call: nothing about the store failed, and the settings file was
/// written. Contract §4 permits `internal` on any command for a condition it
/// does not model, and this is one.
pub fn apply_always_on_top<R: Runtime, M: Manager<R>>(
    manager: &M,
    enabled: bool,
) -> Result<(), ClipError> {
    let mut failed = false;

    for (label, window) in manager.webview_windows() {
        if let Err(error) = window.set_always_on_top(enabled) {
            log::error!("always-on-top could not be applied to window {label}: {error}");
            failed = true;
        }
    }

    if failed {
        Err(ClipError::Internal)
    } else {
        Ok(())
    }
}
