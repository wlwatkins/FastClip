use anyhow::Result;
use commands::del_clip;
use commands::delay_clear_clipboard;
use commands::get_clips;
use commands::load;
use commands::load_db;
use commands::lock;
use commands::new_clip;
use commands::save;
use commands::unlock;
use commands::update_clip;
use structures::DataBase;
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::Mutex;
use tray::setup_tray;

mod commands;
mod password;
mod structures;
mod tray;


async fn update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
      let mut downloaded = 0;
  
      // alternatively we could also call update.download() and update.install() separately
      update
        .download_and_install(
          |chunk_length, content_length| {
            downloaded += chunk_length;
            log::info!("downloaded {downloaded} from {content_length:?}");
          },
          || {
            log::info!("download finished");
          },
        )
        .await?;
  
      log::info!("update installed");
      app.restart();
    }
  
    Ok(())
  }



#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> Result<()> {
    let mut builder =
        tauri::Builder::default().plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.get_webview_window("main").unwrap().set_focus();
        }));
    }

    builder
        .plugin(tauri_plugin_fs::init())
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

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
              update(handle).await.unwrap();
            });


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
            unlock,
            lock,
            load_db,
            delay_clear_clipboard
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
