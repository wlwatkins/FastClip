use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;
lazy_static! {
    static ref DB: Arc<Mutex<DataBase>> = Arc::new(Mutex::new(DataBase::new()));
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: Uuid,
    value: String,
    label: String,
    icon: String,
    colour: String,
    visible: bool,
    clear_time: u64, // using u64 for clear_time, assuming it's a number like TypeScript's `number`
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct DataBase {
    data: HashMap<Uuid, Clip>,
    #[serde(skip)]
    path: PathBuf,
}

impl DataBase {
    pub fn new() -> Self {

        let mut db = Self::default();

        let config_dir = dirs::config_local_dir().expect("Cannot determine config directory");
        let folder = config_dir.join("FastClip");

        if !folder.exists() {
            fs::create_dir_all(&folder).expect("Failed to create FastClip directory");
        }

        db.path = folder.join("db");

        match db.path.exists() {
            false => db.save().expect("Could not save database to disk"),
            true => db.load().expect("Could not load existing db"),
        }
        

        db
    }

    /// Saves the current instance of `AppData` to disk using JSON serialization.
    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&self.path, json)?;
        Ok(())
    }

    /// Loads the `AppData` instance from disk by deserializing the JSON file.
    pub fn load(&mut self) -> Result<()> {
        let json = fs::read_to_string(&self.path)?;
        let db: Self = serde_json::from_str(&json)?;
        self.data = db.data;
        Ok(())
    }
    fn to_vec(&self) -> Result<Vec<Clip>> {
        let clips: Vec<Clip> = self.data.iter().map(|(_, item)| item.clone()).collect();
        Ok(clips)
    }

    fn insert_or_update_clip(&mut self, clip: Clip) -> Result<()> {
        self.data.insert(clip.id, clip);
        self.save()?;
        Ok(())
    }
    fn remove_clip(&mut self, clip_id: Uuid) -> Result<()> {
        self.data
            .remove(&clip_id)
            .ok_or_else(|| anyhow!("Clip ID '{}' not found", clip_id))?;
        self.save()?;

        Ok(())
    }
}

pub async fn insert(clip: Clip) -> Result<()> {
    let mut db = DB.lock().await;
    db.insert_or_update_clip(clip)?;
    Ok(())
}

pub async fn update(clip: Clip) -> Result<()> {
    let mut db = DB.lock().await;
    db.insert_or_update_clip(clip)?;
    Ok(())
}

pub async fn remove(clip_id: Uuid) -> Result<()> {
    let mut db = DB.lock().await;
    db.remove_clip(clip_id)?;
    Ok(())
}

pub async fn get_clips_to_vec() -> Result<Vec<Clip>> {
    let db = DB.lock().await;
    let clips = db.to_vec()?;
    Ok(clips)
}
