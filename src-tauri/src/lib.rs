use std::sync::Arc;

use anyhow::Result;
use lazy_static::lazy_static;
use tauri::AppHandle;
use tokio::sync::Mutex;
use commands::{get_clips, new_clip, update_clip, del_clip};

mod structures;
mod commands;

lazy_static! {
    pub static ref APP_HANDLE: Arc<Mutex<Option<AppHandle>>> = Arc::new(Mutex::new(None));
}


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone(); // Clone `AppHandle` before moving it
            let global_handle = APP_HANDLE.clone();
            
            tauri::async_runtime::spawn(async move {
                let mut handle_lock = global_handle.lock().await;
                *handle_lock = Some(app_handle); // Store cloned handle
            });

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
