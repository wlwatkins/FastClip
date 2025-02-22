use anyhow::Result;
use commands::del_clip;
use commands::get_clips;
use commands::new_clip;
use commands::load;
use commands::update_clip;
use structures::DataBase;
use tauri::Manager;
use tokio::sync::Mutex;
use tray::setup_tray;
mod commands;
mod structures;
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> Result<()> {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // #[cfg(desktop)]
    // {
    //     builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args,
    // _cwd| {         let _ = app
    //             .get_webview_window("main").unwrap()
    //             .set_focus();
    //     }));
    // }

    builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Mutex::new(DataBase::new()));
            setup_tray(app.handle())?;
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
            del_clip,
            load
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
