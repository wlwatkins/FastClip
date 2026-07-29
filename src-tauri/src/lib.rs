//! FastClip's backend.
//!
//! The IPC surface is defined by `docs/src/architecture/contract.md` and is
//! implemented in WP-05 onward. This package (WP-03) provides the store the
//! commands will run against.

pub mod error;
pub mod storage;

pub use error::ClipError;
pub use storage::Store;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Startup recovery runs here, before the event loop serves the
            // webview, so no command can be invoked against a store that has not
            // been recovered. The pre-refactor build populated its global handle
            // inside a task spawned from `setup` and lost that race.
            //
            // The store is Tauri-managed state rather than a `lazy_static`
            // singleton, so a test can build one over a temporary directory.
            app.manage(Store::open_default());
            Ok(())
        });
    // WP-05 registers the command surface here with `invoke_handler`. There is
    // no handler yet: the pre-refactor commands were removed with the type they
    // operated on, and the contract renames all four of them.

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
