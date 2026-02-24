use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::types::AgentEntry;

pub struct AgentStore {
    path: PathBuf,
    active_path: PathBuf,
    agents: Vec<AgentEntry>,
    active_agent_id: Option<String>,
}

impl AgentStore {
    pub fn load(nexus_dir: &Path) -> Result<Self> {
        let path = nexus_dir.join("agents.json");
        let active_path = nexus_dir.join("active_agent.json");

        let agents: Vec<AgentEntry> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            Vec::new()
        };

        let active_agent_id: Option<String> = if active_path.exists() {
            let content = fs::read_to_string(&active_path)?;
            let val: serde_json::Value = serde_json::from_str(&content)?;
            val["agent_id"].as_str().map(|s| s.to_string())
        } else {
            None
        };

        Ok(Self {
            path,
            active_path,
            agents,
            active_agent_id,
        })
    }

    pub fn list(&self) -> &[AgentEntry] {
        &self.agents
    }

    pub fn get(&self, id: &str) -> Option<&AgentEntry> {
        self.agents.iter().find(|a| a.id == id)
    }

    pub fn active_agent_id(&self) -> Option<&str> {
        self.active_agent_id.as_deref()
    }

    pub fn set_active(&mut self, id: Option<String>) -> Result<()> {
        self.active_agent_id = id;
        self.save_active()?;
        Ok(())
    }

    pub fn create(
        &mut self,
        name: String,
        provider_id: String,
        model: String,
        system_prompt: Option<String>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<AgentEntry> {
        let now = Utc::now();
        let agent = AgentEntry {
            id: Uuid::new_v4().to_string(),
            name,
            provider_id,
            model,
            system_prompt,
            temperature,
            max_tokens,
            created_at: now,
            updated_at: now,
        };
        self.agents.push(agent.clone());
        self.save()?;
        Ok(agent)
    }

    pub fn update(&mut self, id: &str, updates: AgentUpdate) -> Result<Option<AgentEntry>> {
        let Some(agent) = self.agents.iter_mut().find(|a| a.id == id) else {
            return Ok(None);
        };

        if let Some(name) = updates.name {
            agent.name = name;
        }
        if let Some(provider_id) = updates.provider_id {
            agent.provider_id = provider_id;
        }
        if let Some(model) = updates.model {
            agent.model = model;
        }
        if updates.system_prompt.is_some() {
            agent.system_prompt = updates.system_prompt;
        }
        if updates.set_temperature {
            agent.temperature = updates.temperature;
        }
        if updates.set_max_tokens {
            agent.max_tokens = updates.max_tokens;
        }
        agent.updated_at = Utc::now();

        let updated = agent.clone();
        self.save()?;
        Ok(Some(updated))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let len = self.agents.len();
        self.agents.retain(|a| a.id != id);
        if self.agents.len() < len {
            // Clear active if deleted agent was active
            if self.active_agent_id.as_deref() == Some(id) {
                self.active_agent_id = None;
                self.save_active()?;
            }
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self.agents)?;
        fs::write(&self.path, content)?;
        Ok(())
    }

    fn save_active(&self) -> Result<()> {
        let content = serde_json::json!({ "agent_id": self.active_agent_id });
        fs::write(&self.active_path, serde_json::to_string_pretty(&content)?)?;
        Ok(())
    }
}

pub struct AgentUpdate {
    pub name: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub set_temperature: bool,
    pub max_tokens: Option<u32>,
    pub set_max_tokens: bool,
}
