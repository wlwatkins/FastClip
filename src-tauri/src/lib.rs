use anyhow::Result;
use commands::del_clip;
use commands::get_clips;
use commands::load;
use commands::new_clip;
use commands::save;
use commands::update_clip;
use commands::delay_clear_clipboard;
use structures::DataBase;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tokio::sync::Mutex;
use tray::setup_tray;

mod commands;
mod structures;
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> Result<()> {
    let builder = tauri::Builder::default();
    // #[cfg(desktop)]
    // {
    //     builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args,
    // _cwd| {         let _ = app
    //             .get_webview_window("main").unwrap()
    //             .set_focus();
    //     }));
    // }

    builder
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Mutex::new(DataBase::new()));
            setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            new_clip,
            update_clip,
            get_clips,
            del_clip,
            load,
            save,
            delay_clear_clipboard
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
