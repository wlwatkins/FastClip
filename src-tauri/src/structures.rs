use anyhow::anyhow;
use anyhow::Result;

use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id:     Uuid,
    value:      String,
    label:      String,
    icon:       String,
    colour:     String,
    visible:    bool,
    clear_time: Option<u64>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct DataBase {
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

    pub fn to_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(&path, json)?;
        Ok(())
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(&path)?;
        let db: Self = serde_json::from_str(&json)?;
        let mut new_db = Self::new();
        new_db.data = db.data;
        Ok(new_db)
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn load(&mut self) -> Result<()> {
        let json = fs::read_to_string(&self.path)?;
        let db: Self = serde_json::from_str(&json)?;
        self.data = db.data;
        Ok(())
    }

    pub fn to_vec(&self) -> Result<Vec<Clip>> {
        let clips: Vec<Clip> = self.data.values().cloned().collect();
        Ok(clips)
    }

    pub fn insert_or_update_clip(
        &mut self,
        clip: Clip,
    ) -> Result<()> {
        self.data.insert(clip.id, clip);
        self.save()?;
        Ok(())
    }
    pub fn remove_clip(
        &mut self,
        clip_id: Uuid,
    ) -> Result<()> {
        self.data
            .remove(&clip_id)
            .ok_or_else(|| anyhow!("Clip ID '{}' not found", clip_id))?;
        self.save()?;

        Ok(())
    }
}
