use anyhow::Result;
use commands::{del_clip, get_clips, new_clip, update_clip};
use structures::DataBase;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

mod commands;
mod structures;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> Result<()> {
    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app
                .get_webview_window("main")
                .expect("no main window")
                .set_focus();
        }));
    }

    builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Mutex::new(DataBase::new()));
            // let app_handle = app.handle().clone(); // Clone `AppHandle` before moving it
            // let global_handle = APP_HANDLE.clone();

            // tauri::async_runtime::spawn(async move {
            //     let mut handle_lock = global_handle.lock().await;
            //     *handle_lock = Some(app_handle); // Store cloned handle
            // });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            new_clip,
            update_clip,
            get_clips,
            del_clip
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
