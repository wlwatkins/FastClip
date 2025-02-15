use crate::{structures::Clip, APP_HANDLE};
use tauri::{command, Emitter, Error};
use anyhow::{anyhow, Result};
use uuid::Uuid;
use crate::structures::insert;
use crate::structures::update;
use crate::structures::remove;
use crate::structures::get_clips_to_vec;

#[command(rename_all = "snake_case")]
pub async fn new_clip(clip: Clip) -> Result<(), Error> {
    println!("new_clip {:?}", clip);
    insert(clip).await?;
    update_frontend().await?;
    Ok(())
}


#[command(rename_all = "snake_case")]
pub async fn update_clip(clip: Clip) -> Result<(), Error> {
    println!("update_clip {:?}", clip);
    update(clip).await?;
    update_frontend().await?;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn del_clip(clip_id: String) -> Result<(), Error> {
    println!("del_clip {:?}", clip_id);
    let uuid = Uuid::parse_str(&clip_id).map_err(|e| anyhow!("Invalid UUID: {}", e))?;
    remove(uuid).await?;
    update_frontend().await?;
    Ok(())
}


#[command(rename_all = "snake_case")]
pub async fn get_clips() -> Result<Vec<Clip>, Error> {
    let clips = get_clips_to_vec().await?;
    Ok(clips)
}


pub async fn update_frontend()  -> Result<(), Error> { // Handle the logic
    let clips = get_clips_to_vec().await?;
    let handle_lock = APP_HANDLE.lock().await;
    if let Some(app) = handle_lock.as_ref() {  // Correct `if let` syntax
        app.emit("update_clips", clips)?;  // Use `emit`
    }

    Ok(())
    
}