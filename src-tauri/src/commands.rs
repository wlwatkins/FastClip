use crate::{structures::Clip, APP_HANDLE};
use tauri::{command, Emitter, Error};
use anyhow::{anyhow, Result};
use uuid::Uuid;
use crate::DB;

#[command(rename_all = "snake_case")]
pub async fn new_clip(clip: Clip) -> Result<(), Error> {
    // Handle the logic
    println!("new_clip {:?}", clip);

    {
        let mut db = DB.lock().await;
        db.insert(clip.id, clip);
    }

    update_frontend().await?;

    Ok(())
}


#[command(rename_all = "snake_case")]
pub async fn update_clip(clip: Clip) -> Result<(), Error> {
    // Handle the logic
    println!("update_clip {:?}", clip);

    {
        let mut db = DB.lock().await;
        db.insert(clip.id, clip);
    }

    update_frontend().await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn del_clip(clip_id: String) -> Result<(), Error> {
    println!("del_clip {:?}", clip_id);
    let uuid = Uuid::parse_str(&clip_id).map_err(|e| anyhow!("Invalid UUID: {}", e))?;

    {
        let mut db = DB.lock().await;
        db.remove(&uuid).ok_or_else(|| anyhow!("Clip ID '{}' not found", clip_id))?;
    }

    update_frontend().await?;

    Ok(())
}


#[command(rename_all = "snake_case")]
pub async fn get_clips() -> Result<Vec<Clip>, Error> {
    let clips: Vec<Clip> = {
        let db = DB.lock().await;
        println!("get_clips {:?}", db);
        db.iter().map(|(_, item)| item.clone()).collect()
    };
    Ok(clips)
}


pub async fn update_frontend()  -> Result<(), Error> { // Handle the logic

    let clips: Vec<Clip> = {
        let db = DB.lock().await;
        db.iter().map(|(_, item)| item.clone()).collect()
    };

    let handle_lock = APP_HANDLE.lock().await;
    if let Some(app) = handle_lock.as_ref() {  // Correct `if let` syntax
        app.emit("update_clips", clips)?;  // Use `emit`
    }

    Ok(())
    
}