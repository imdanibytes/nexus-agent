use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::types::AgentEntry;
use crate::config::NexusConfig;

pub struct AgentStore {
    agents: Vec<AgentEntry>,
    active_agent_id: Option<String>,
}

impl AgentStore {
    pub fn new(agents: Vec<AgentEntry>, active_agent_id: Option<String>) -> Self {
        Self {
            agents,
            active_agent_id,
        }
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
        self.save()
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
        self.create_with_mcp(name, provider_id, model, system_prompt, temperature, max_tokens, None)
    }

    pub fn create_with_mcp(
        &mut self,
        name: String,
        provider_id: String,
        model: String,
        system_prompt: Option<String>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        mcp_server_ids: Option<Vec<String>>,
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
            thinking_budget: None,
            mcp_server_ids,
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
        if updates.set_thinking_budget {
            agent.thinking_budget = updates.thinking_budget;
        }
        if updates.set_mcp_server_ids {
            agent.mcp_server_ids = updates.mcp_server_ids;
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
            }
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save(&self) -> Result<()> {
        let mut config = NexusConfig::load()?;
        config.agents = self.agents.clone();
        config.active_agent_id = self.active_agent_id.clone();
        config.save()
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
    pub thinking_budget: Option<u32>,
    pub set_thinking_budget: bool,
    pub mcp_server_ids: Option<Vec<String>>,
    pub set_mcp_server_ids: bool,
}
