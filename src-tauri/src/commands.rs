use std::path::PathBuf;

use crate::structures::Clip;
use crate::structures::DataBase;
use anyhow::anyhow;
use anyhow::Result;
use tauri::command;
use tauri::Emitter;
use tauri::Error;
use tauri::State;
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
    db.insert_or_update_clip(clip)?;
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
    db.insert_or_update_clip(clip)?;
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
    db.remove_clip(uuid)?;
    app_handle.emit("update_clips", db.to_vec()?)?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn get_clips(state: State<'_, Mutex<DataBase>>) -> Result<Vec<Clip>, Error> {
    let db = state.lock().await;
    let clips = db.to_vec()?;
    Ok(clips)
}

#[command(rename_all = "snake_case")]
pub async fn load(state: State<'_, Mutex<DataBase>>, file: PathBuf) -> Result<(), Error> {
    let mut db = state.lock().await;
    *db = DataBase::from_path(file)?;
    Ok(())
}
