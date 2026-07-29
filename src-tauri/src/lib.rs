//! FastClip's backend.
//!
//! The IPC surface is defined by `docs/src/architecture/contract.md` and is
//! implemented in [`commands`]. WP-03 provided the store; WP-05 provides the
//! five clip commands that run against it.

pub mod colour;
pub mod commands;
pub mod error;
pub(crate) mod redact;
pub mod storage;

pub use colour::Colour;
pub use error::ClipError;
pub use storage::Store;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        // Registered on the builder, so the surface exists before the event
        // loop can deliver an `invoke`. Contract §2 names sixteen commands;
        // these five are WP-05's, and a command that is not implemented is not
        // registered — invoking one rejects rather than appearing to work.
        .invoke_handler(tauri::generate_handler![
            commands::clips::list_clips,
            commands::clips::create_clip,
            commands::clips::update_clip,
            commands::clips::delete_clip,
            commands::clips::copy_clip,
        ])
        // Startup recovery runs here, before the application is built, so no
        // command can be invoked against a store that has not been recovered.
        // The pre-refactor build populated its global handle inside a task
        // spawned from `setup` and lost that race.
        //
        // **`manage` on the builder, not inside `setup`.** Tauri runs the
        // `setup` hook from `App::run`, not from `Builder::build`, so a store
        // managed there does not exist for the whole interval between the two —
        // and a command invoked in that interval fails with a plain string
        // saying the state is not managed, which is not a `ClipError` and which
        // contract §0 forbids. Registering it on the builder closes the window
        // rather than reasoning about how narrow it is.
        //
        // The store is Tauri-managed state rather than a `lazy_static`
        // singleton, so a test can build one over a temporary directory.
        .manage(Store::open_default());

    let app = match builder.build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(error) => {
            log::error!("FastClip could not start: {error}");
            return;
        }
    };

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
