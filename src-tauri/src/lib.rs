//! FastClip's backend.
//!
//! The IPC surface is defined by `docs/src/architecture/contract.md` and is
//! implemented in [`commands`]. WP-03 provided the store; WP-05 provides the
//! five clip commands that run against it; WP-06 adds `reorder_clips`; WP-09
//! adds `export_clips` and `import_clips`; WP-14 provides the settings pair,
//! `get_lock_state`, and the window this file creates. WP-08 adds the tray in
//! [`tray`], which serves no IPC command at all — it calls the same copy
//! function `copy_clip` does (ADR-0008).

pub mod colour;
pub mod commands;
pub mod crypto;
pub mod error;
pub mod export_file;
pub(crate) mod redact;
pub mod settings;
pub mod storage;
pub mod tray;
pub mod window;

pub use colour::Colour;
pub use error::ClipError;
pub use settings::Settings;
pub use storage::Store;

use tauri::plugin::TauriPlugin;
use tauri::{Manager, RunEvent};

/// The log sink [ADR-0012](../../docs/src/architecture/adr/0012-logging.md)
/// specifies: stdout and a rotating file, `Info` in release and `Debug` in
/// development, **no webview target** — a log line is not a second channel
/// across the seam the contract does not describe.
///
/// **The content rule is what makes this safe to turn on at all**: no
/// `log::` call anywhere in this crate may format a clip `label` or `value`
/// (ADR-0012), so nothing this sink writes needs to be treated as sensitive
/// beyond ordinary application diagnostics.
///
/// Factored out of [`run`] so a test can build the identical plugin against a
/// temporary directory rather than reimplementing its construction — the
/// thing under test is this crate's wiring, not a second copy of it.
///
/// `log_dir` is `None` only when [`dirs::home_dir`] itself failed, which
/// [`storage::paths::StorePaths::from_home`] already treats as a `storage`
/// fault elsewhere. Refusing to log at all over a missing home directory would
/// make the one fault a user in that state could report the one fault nobody
/// could see, so stdout stays live either way.
pub fn log_plugin<R: tauri::Runtime>(log_dir: Option<std::path::PathBuf>) -> TauriPlugin<R> {
    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let mut builder = tauri_plugin_log::Builder::new()
        .level(level)
        // The plugin's own defaults already exclude the webview target, but
        // `clear_targets` makes that a fact this function states rather than
        // one it inherits — ADR-0012 calls the webview target off load-bearing,
        // not incidental.
        .clear_targets()
        .target(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::Stdout,
        ));

    if let Some(dir) = log_dir {
        builder = builder.target(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::Folder {
                path: dir,
                // Named explicitly rather than left to the plugin's
                // `app_name.clone()` default, so the file this function writes
                // and the file a test greps are provably the same path.
                file_name: Some("fast-clip".into()),
            },
        ));
    }

    builder.build()
}

/// **The IPC command list, in one place.** Expands to the
/// `tauri::generate_handler!` invocation that registers every implemented
/// command; [`run`] and `tests/ipc.rs` both call it, so the application and the
/// suite that checks the application cannot describe different surfaces.
///
/// There used to be a second literal copy of this list in `tests/ipc.rs`, and it
/// was already wrong: WP-06 found the suite asserting `reorder_clips` was *not*
/// registered, so it passed while describing a surface that had changed. Nothing
/// made the two agree, and nothing was going to.
///
/// **Why a `macro_rules!` that wraps a proc macro, and not the reverse.**
/// `generate_handler!` takes paths, resolves them at its own call site, and
/// cannot see through a macro call nested inside its argument list — so wrapping
/// it the other way round does not work. This expands *before* the proc macro
/// runs, handing it the same token stream it would have been written with. The
/// paths are `$crate::…` so they resolve to this crate from an integration test,
/// where a bare `commands::…` would not resolve at all.
///
/// **Adding a command means adding one line here.** It then appears in the
/// application and in every test that builds one, which is what makes
/// `tests/ipc.rs`'s "not implemented yet" assertions fail loudly for a command
/// that has since been registered.
#[macro_export]
macro_rules! command_handler {
    () => {
        ::tauri::generate_handler![
            $crate::commands::clips::list_clips,
            $crate::commands::clips::create_clip,
            $crate::commands::clips::update_clip,
            $crate::commands::clips::delete_clip,
            $crate::commands::clips::copy_clip,
            $crate::commands::clips::reorder_clips,
            $crate::commands::encryption::enable_encryption,
            $crate::commands::encryption::disable_encryption,
            $crate::commands::encryption::change_pin,
            $crate::commands::lock::unlock,
            $crate::commands::lock::lock,
            $crate::commands::export_import::export_clips,
            $crate::commands::export_import::import_clips,
            $crate::commands::lock::get_lock_state,
            $crate::commands::settings::get_settings,
            $crate::commands::settings::set_always_on_top,
        ]
    };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // **Registered first and built before anything below runs.** A
        // plugin's `setup` runs inside `Builder::build`, not `App::run` —
        // unlike the `Builder::setup` closure hook, which waits for the
        // latter (see the module header of `tests/ipc.rs` for that distinction
        // costing a debugging cycle once already). That is what makes it safe
        // to open the store immediately after `build()` returns below: the
        // global logger this plugin installs is already live by then, so
        // startup recovery's lines are recorded rather than discarded
        // (ADR-0012, WP-02 addendum). Before this reordering, `Store::open_default()`
        // ran before the builder even existed, and every startup-recovery
        // `log::` line — including `colour.rs`'s only statement of the
        // pre-WP-10 remedy — was discarded no matter what sink `run` went on
        // to install.
        .plugin(log_plugin(
            dirs::home_dir().map(|home| home.join(storage::paths::DIR_NAME)),
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        // **The Rust half of the export and import file dialogs (WP-09).**
        // Contract closed question 15 gives the dialog to the frontend, which
        // calls `@tauri-apps/plugin-dialog`'s `save()` and `open()` — and those
        // reject at runtime unless this line runs and the capability grants
        // `dialog:allow-save` and `dialog:allow-open`. Nothing in the JavaScript
        // toolchain catches its absence: `tsc`, `svelte-check`, `vitest` and
        // `vite build` all pass without a Tauri runtime, so the failure would
        // first appear to the user as a dialog that never opens.
        .plugin(tauri_plugin_dialog::init())
        // Registered on the builder, so the surface exists before the event
        // loop can deliver an `invoke`. Contract §2 names sixteen commands;
        // the eleven in [`command_handler`] are WP-05's, WP-06's, WP-09's and
        // WP-14's, and a command that is not implemented is not registered —
        // invoking one rejects rather than appearing to work.
        //
        // The list itself lives in the macro so that `tests/ipc.rs` registers
        // the same one. It is the only copy.
        .invoke_handler(command_handler!());

    let app = match builder.build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(error) => {
            log::error!("FastClip could not start: {error}");
            return;
        }
    };

    // Startup recovery, now that the log sink installed above is live — so its
    // lines are recorded rather than discarded (ADR-0012). It resolves
    // `~/.fast-clip/` and leaves the store either open or holding a recorded
    // fault; it never fails the launch, because a backend that refuses to
    // start leaves nothing to show the user.
    let store = Store::open_default();

    // **Read before the window exists.** Applying always-on-top at creation is
    // what stops the window appearing in the wrong state and then correcting
    // itself, and it is why the frontend must not call Tauri's window API for
    // this (contract, `set_always_on_top`). A store directory that could not be
    // reached is not a reason to refuse to open a window: the default is the
    // documented fallback and `get_lock_state` reports the fault.
    let persisted = match store.directory() {
        Ok(paths) => settings::read(paths),
        Err(_) => Settings::default(),
    };

    // **`manage` on the built `App`, immediately, not inside a `setup` hook.**
    // Tauri runs the `Builder::setup` closure hook from `App::run`, not from
    // `Builder::build`, so a store managed there would not exist for the whole
    // interval between the two — and a command invoked in that interval fails
    // with a plain string saying the state is not managed, which is not a
    // `ClipError` and which contract §0 forbids. This call happens synchronously
    // here, before `window::create_main` and long before `app.run` below, so
    // that interval still does not exist; only *where* the store is opened
    // moved, to let the plugin above see the log sink installed first.
    //
    // The store is Tauri-managed state rather than a `lazy_static` singleton,
    // so a test can build one over a temporary directory.
    app.manage(store);

    // The main window carries `"create": false` in `tauri.conf.json`, so Tauri
    // does not build it during its own setup and this call is the only thing
    // that does. It is here rather than in a `setup` hook for the same reason
    // `app.manage` above is: `Builder::setup` fires from `App::run`, not from
    // `Builder::build`, so anything this application needs before the event loop
    // starts belongs on the built `App` where it can be seen — not in a hook
    // that has not run yet. The runtime and its event loop exist from `build()`,
    // which is what makes this the earliest point a window can be made — and it
    // was checked by running the application, not by reasoning about it.
    //
    // **A failure stops the process, exactly as a failed `build()` does eleven
    // lines above.** Entering `app.run` with zero windows is not a degraded
    // application, it is an invisible one: Tauri raises `ExitRequested` only
    // when the last window is *destroyed*, and none was created, so the event
    // loop never terminates. With `windows_subsystem = "windows"` there is no
    // console either, so the user's only remedy would be Task Manager and every
    // retry would add another orphan.
    if !window::create_main(&app, persisted.always_on_top) {
        // Checkpoint and close on the way out, on the same reasoning as the
        // `Exit` handler below. Nothing was written this session, but startup
        // recovery opened the connection and may have created the schema.
        if let Some(store) = app.try_state::<Store>() {
            store.shutdown();
        }
        return;
    }

    // **After the window, and a failure does not stop the launch** (WP-08). The
    // tray's *Unlock FastClip* item raises the window, so the window must exist
    // first; and an application with a window and no tray is degraded, while one
    // with neither is invisible. `install` logs whatever went wrong.
    tray::install(app.handle());

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Checkpoint the WAL and close, so the sidecars do not outlive the
            // process. A failure here is absorbed; the committed data is in the
            // WAL either way (ADR-0009).
            if let Some(store) = handle.try_state::<Store>() {
                store.shutdown();
            }
        }
    });
}
