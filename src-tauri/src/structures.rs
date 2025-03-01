// use aes_gcm::Aes256Gcm;
// use aes_gcm::KeyInit;
// use aes_gcm::Nonce;
// use aes_gcm::aead::Aead;
// use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
// use rand::RngCore;
// use rand::rngs::ThreadRng;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

// use crate::password::PASSWORD;

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
            std::fs::create_dir_all(&folder).expect("Failed to create FastClip directory");
        }

        db.path = folder.join("db");

        // match db.path.exists() {
        //     false => db.save().expect("Could not save database to disk"),
        //     true => db.load().expect("Could not load existing db"),
        // }

        db
    }

    pub async fn to_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(&path, json).await?;
        Ok(())
    }

    pub async fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json = fs::read_to_string(&path).await?;
        let db: Self = serde_json::from_str(&json)?;
        let mut new_db = Self::new();
        new_db.data = db.data;
        Ok(new_db)
    }

    pub async fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&self.path, json).await?;
        Ok(())
    }

    pub async fn load(&mut self) -> Result<()> {
        let json = fs::read_to_string(&self.path).await?;
        let db: Self = serde_json::from_str(&json)?;
        self.data = db.data;
        Ok(())
    }

    // pub async fn save(
    //     &self,
    // ) -> Result<()> {

    // let key = match *PASSWORD.lock().await {
    //     Some(ref key) => key.clone(),
    //     None => return Err(anyhow!("Database not unlocked")),
    // };

    // let json = serde_json::to_string_pretty(self)?;

    // let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|e|
    // anyhow!(e))?;

    // let mut rng: ThreadRng = rand::rng();

    // let mut nonce_bytes = [0u8; 12]; // 96-bit nonce
    // rng.fill_bytes(&mut nonce_bytes); // Fill the nonce with random data

    // let nonce = Nonce::from_slice(&nonce_bytes);

    // let ciphertext = cipher
    //     .encrypt(nonce, json.as_bytes())
    //     .map_err(|e| anyhow!(e))?;

    // // Store nonce with ciphertext
    // let mut file_contents = nonce_bytes.to_vec();
    // file_contents.extend(ciphertext);

    // Write the file contents to disk
    // fs::write(&self.path, json)?;

    // Ok(())
    // }

    // pub async fn load(&mut self) -> Result<()> {

    // let key = match *PASSWORD.lock().await {
    //     Some(ref key) => key.clone(),
    //     None => return Err(anyhow!("Database not unlocked")),
    // };

    // Read the file contents
    // let file_contents = fs::read(&self.path).context("Failed to read file")?;

    // let (nonce, ciphertext) = file_contents.split_at(12);

    // let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|e|
    // anyhow!(e))?;

    // let plaintext = cipher
    //     .decrypt(Nonce::from_slice(nonce), ciphertext)
    //     .map_err(|e| anyhow!(e))?;

    // Deserialize the plaintext into the database structure
    // let db: Self = serde_json::from_slice(&file_contents)?;

    // Update the data field of the current object
    //     self.data = db.data;

    //     Ok(())
    // }

    pub fn to_vec(&self) -> Result<Vec<Clip>> {
        let clips: Vec<Clip> = self.data.values().cloned().collect();
        Ok(clips)
    }

    pub async fn insert_or_update_clip(
        &mut self,
        clip: Clip,
    ) -> Result<()> {
        self.data.insert(clip.id, clip);
        self.save().await?;
        Ok(())
    }
    pub async fn remove_clip(
        &mut self,
        clip_id: Uuid,
    ) -> Result<()> {
        self.data
            .remove(&clip_id)
            .ok_or_else(|| anyhow!("Clip ID '{}' not found", clip_id))?;

        self.save().await?;

        Ok(())
    }
}
