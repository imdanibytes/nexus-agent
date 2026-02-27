use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::config::{NexusConfig, Project};

/// Manages CRUD for project configurations (persisted to nexus.json).
pub struct ProjectStore {
    projects: Vec<Project>,
}

impl ProjectStore {
    pub fn new(projects: Vec<Project>) -> Self {
        Self { projects }
    }

    pub fn list(&self) -> &[Project] {
        &self.projects
    }

    pub fn get(&self, id: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn create(&mut self, name: String, path: String) -> Result<Project> {
        let now = Utc::now().to_rfc3339();
        let project = Project {
            id: Uuid::new_v4().to_string(),
            name,
            path,
            created_at: now.clone(),
            updated_at: now,
        };
        self.projects.push(project.clone());
        self.save()?;
        Ok(project)
    }

    pub fn update(
        &mut self,
        id: &str,
        updates: ProjectUpdate,
    ) -> Result<Option<Project>> {
        let Some(proj) = self.projects.iter_mut().find(|p| p.id == id) else {
            return Ok(None);
        };

        if let Some(name) = updates.name {
            proj.name = name;
        }
        if let Some(path) = updates.path {
            proj.path = path;
        }
        proj.updated_at = Utc::now().to_rfc3339();

        let updated = proj.clone();
        self.save()?;
        Ok(Some(updated))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let len = self.projects.len();
        self.projects.retain(|p| p.id != id);
        if self.projects.len() < len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Persist projects back to nexus.json via read-modify-write.
    fn save(&self) -> Result<()> {
        let mut config = NexusConfig::load()?;
        config.projects = self.projects.clone();
        config.save()
    }
}

pub struct ProjectUpdate {
    pub name: Option<String>,
    pub path: Option<String>,
}
