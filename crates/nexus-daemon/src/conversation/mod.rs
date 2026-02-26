pub mod types;

use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

pub use types::*;

pub struct ConversationStore {
    base_dir: PathBuf,
    index: Vec<ConversationMeta>,
}

impl ConversationStore {
    pub fn load(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&base_dir)?;

        let index_path = base_dir.join("index.json");
        let index = if index_path.exists() {
            let content = fs::read_to_string(&index_path)?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };

        Ok(Self { base_dir, index })
    }

    pub fn list(&self) -> &[ConversationMeta] {
        &self.index
    }

    pub fn create(&mut self, client_id: Option<String>) -> Result<ConversationMeta> {
        let id = client_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Utc::now();
        let meta = ConversationMeta {
            id: id.clone(),
            title: "New Chat".to_string(),
            created_at: now,
            updated_at: now,
            message_count: 0,
        };

        let conv = Conversation {
            id: id.clone(),
            title: "New Chat".to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            active_path: Vec::new(),
            usage: None,
            agent_id: None,
            spans: Vec::new(),
        };

        self.write_conversation(&conv)?;
        self.index.push(meta.clone());
        self.save_index()?;

        Ok(meta)
    }

    pub fn get(&self, id: &str) -> Result<Option<Conversation>> {
        let path = self.conv_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let conv: Conversation = serde_json::from_str(&content)?;
        Ok(Some(conv))
    }

    pub fn save(&mut self, conv: &Conversation) -> Result<()> {
        self.write_conversation(conv)?;

        // Update index
        if let Some(meta) = self.index.iter_mut().find(|m| m.id == conv.id) {
            meta.title = conv.title.clone();
            meta.updated_at = conv.updated_at;
            meta.message_count = conv.messages.len();
        }
        self.save_index()?;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        let path = self.conv_path(id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        self.index.retain(|m| m.id != id);
        self.save_index()?;
        Ok(())
    }

    pub fn rename(&mut self, id: &str, title: &str) -> Result<()> {
        if let Some(mut conv) = self.get(id)? {
            conv.title = title.to_string();
            conv.updated_at = Utc::now();
            self.write_conversation(&conv)?;

            if let Some(meta) = self.index.iter_mut().find(|m| m.id == id) {
                meta.title = title.to_string();
                meta.updated_at = conv.updated_at;
            }
            self.save_index()?;
        }
        Ok(())
    }

    fn conv_path(&self, id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", id))
    }

    fn write_conversation(&self, conv: &Conversation) -> Result<()> {
        let path = self.conv_path(&conv.id);
        let content = serde_json::to_string_pretty(conv)?;
        fs::write(&path, content)?;
        Ok(())
    }

    fn save_index(&self) -> Result<()> {
        let path = self.base_dir.join("index.json");
        let content = serde_json::to_string_pretty(&self.index)?;
        fs::write(&path, content)?;
        Ok(())
    }
}
