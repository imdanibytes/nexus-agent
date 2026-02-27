use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::config::{NexusConfig, Workspace};

/// Manages CRUD for logical workspace groupings (persisted to nexus.json).
pub struct WorkspaceStore {
    workspaces: Vec<Workspace>,
}

impl WorkspaceStore {
    pub fn new(workspaces: Vec<Workspace>) -> Self {
        Self { workspaces }
    }

    pub fn list(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn get(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn create(
        &mut self,
        name: String,
        description: Option<String>,
        project_ids: Vec<String>,
    ) -> Result<Workspace> {
        let now = Utc::now().to_rfc3339();
        let workspace = Workspace {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            project_ids,
            created_at: now.clone(),
            updated_at: now,
        };
        self.workspaces.push(workspace.clone());
        self.save()?;
        Ok(workspace)
    }

    pub fn update(
        &mut self,
        id: &str,
        updates: WorkspaceUpdate,
    ) -> Result<Option<Workspace>> {
        let Some(ws) = self.workspaces.iter_mut().find(|w| w.id == id) else {
            return Ok(None);
        };

        if let Some(name) = updates.name {
            ws.name = name;
        }
        if updates.description.is_some() {
            ws.description = updates.description;
        }
        if let Some(project_ids) = updates.project_ids {
            ws.project_ids = project_ids;
        }
        ws.updated_at = Utc::now().to_rfc3339();

        let updated = ws.clone();
        self.save()?;
        Ok(Some(updated))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let len = self.workspaces.len();
        self.workspaces.retain(|w| w.id != id);
        if self.workspaces.len() < len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Persist workspaces back to nexus.json.
    fn save(&self) -> Result<()> {
        let mut config = NexusConfig::load()?;
        config.workspaces = self.workspaces.clone();
        config.save()
    }
}

pub struct WorkspaceUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub project_ids: Option<Vec<String>>,
}
