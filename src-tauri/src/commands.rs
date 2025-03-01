use std::path::PathBuf;
use std::time::Duration;

use crate::password::PASSWORD;
use crate::structures::Clip;
use crate::structures::DataBase;
use anyhow::Result;
use anyhow::anyhow;
use log::debug;
use tauri::Emitter;
use tauri::Error;
use tauri::State;
use tauri::command;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::Mutex;
use uuid::Uuid;

#[command(rename_all = "snake_case")]
pub async fn new_clip(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
    clip: Clip,
) -> Result<(), Error> {
    log::debug!("new_clip {:?}", clip);
    let mut db = state.lock().await;
    db.insert_or_update_clip(clip).await?;
    app_handle.emit("update_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn update_clip(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
    clip: Clip,
) -> Result<(), Error> {
    log::debug!("update_clip {:?}", clip);
    let mut db = state.lock().await;
    db.insert_or_update_clip(clip).await?;
    app_handle.emit("update_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn del_clip(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
    clip_id: String,
) -> Result<(), Error> {
    log::debug!("del_clip {:?}", clip_id);
    let uuid = Uuid::parse_str(&clip_id).map_err(|e| anyhow!("Invalid UUID: {}", e))?;
    let mut db = state.lock().await;
    db.remove_clip(uuid).await?;
    app_handle.emit("update_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn get_clips(state: State<'_, Mutex<DataBase>>) -> Result<Vec<Clip>, Error> {
    let mut db = state.lock().await;
    db.load().await?;
    let clips = db.to_vec()?;
    Ok(clips)
}

#[command(rename_all = "snake_case")]
pub async fn load(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
    file: PathBuf,
) -> Result<(), Error> {
    let mut db = state.lock().await;
    *db = DataBase::from_path(file).await?;
    app_handle.emit("update_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn save(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
    file: PathBuf,
) -> Result<(), Error> {
    let db = state.lock().await;
    db.to_path(file).await?;
    app_handle.emit("update_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn delay_clear_clipboard(
    app_handle: tauri::AppHandle,
    delay: u64,
) -> Result<(), Error> {
    let app_clone = app_handle.clone();
    debug!("Starting thread for clearing clipboard in {delay}s");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(delay)).await;
        debug!("Clearing clipboard");
        app_clone
            .clipboard()
            .write_text("".to_string())
            .expect("Could not clear clipboard");
    });

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn unlock(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
    key: Vec<u8>,
) -> Result<(), Error> {
    {
        let mut password = PASSWORD.lock().await;
        *password = Some(key);
    }
    let mut db = state.lock().await;
    db.load().await?;
    app_handle.emit("unlock_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn lock(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
) -> Result<(), Error> {
    let db = state.lock().await;
    db.save().await?;
    {
        let mut password = PASSWORD.lock().await;
        *password = None;
    }
    app_handle.emit("lock_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn load_db(
    state: State<'_, Mutex<DataBase>>,
    app_handle: tauri::AppHandle,
) -> Result<(), Error> {
    let mut db = state.lock().await;
    db.load().await?;
    app_handle.emit("load_clips", db.to_vec()?)?;
    Ok(())
}
