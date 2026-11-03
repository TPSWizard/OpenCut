//! Projects on disk: one folder per project holding `project.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::MediaTime;

use crate::document::{PROJECT_FORMAT_VERSION, Project};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub duration: MediaTime,
    pub created_at: u64,
    pub updated_at: u64,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ProjectStore {
    root: PathBuf,
}

const PROJECT_FILE: &str = "project.json";

impl ProjectStore {
    /// `~/.opencut/projects`, next to the headless runner's profile folder.
    pub fn default_root() -> PathBuf {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".opencut").join("projects")
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn project_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    pub fn project_file(&self, id: &str) -> PathBuf {
        self.project_dir(id).join(PROJECT_FILE)
    }

    pub fn list(&self) -> io::Result<Vec<ProjectSummary>> {
        let mut summaries = Vec::new();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(summaries),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let file = entry.path().join(PROJECT_FILE);
            if !file.is_file() {
                continue;
            }
            match read_project(&file) {
                Ok(project) => summaries.push(ProjectSummary {
                    id: project.id.clone(),
                    name: project.name.clone(),
                    duration: project.duration(),
                    created_at: project.created_at,
                    updated_at: project.updated_at,
                    path: file,
                }),
                // A broken file must not hide every other project.
                Err(error) => eprintln!("skipping {}: {error}", file.display()),
            }
        }
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    pub fn load(&self, id: &str) -> io::Result<Project> {
        read_project(&self.project_file(id))
    }

    pub fn save(&self, project: &Project) -> io::Result<()> {
        let dir = self.project_dir(&project.id);
        fs::create_dir_all(&dir)?;
        let file = dir.join(PROJECT_FILE);
        let temp = dir.join(format!("{PROJECT_FILE}.tmp"));
        let json = serde_json::to_vec_pretty(project).map_err(io::Error::other)?;
        fs::write(&temp, json)?;
        // Replace atomically where the platform allows it.
        if let Err(error) = fs::rename(&temp, &file) {
            fs::remove_file(&file).ok();
            fs::rename(&temp, &file).map_err(|_| error)?;
        }
        Ok(())
    }

    pub fn create(&self, name: &str) -> io::Result<Project> {
        let project = Project::new(name);
        self.save(&project)?;
        Ok(project)
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        let dir = self.project_dir(id);
        if dir.join(PROJECT_FILE).is_file() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

fn read_project(file: &Path) -> io::Result<Project> {
    let bytes = fs::read(file)?;
    let project: Project = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if project.version > PROJECT_FORMAT_VERSION {
        return Err(io::Error::other(format!(
            "project format {} is newer than this app understands ({PROJECT_FORMAT_VERSION})",
            project.version
        )));
    }
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_lists_loads_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path().join("projects"));
        assert!(store.list().unwrap().is_empty());
        let project = store.create("First").unwrap();
        let mut second = store.create("Second").unwrap();
        second.updated_at = project.updated_at + 10;
        store.save(&second).unwrap();

        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Second");
        assert_eq!(store.load(&project.id).unwrap(), project);

        fs::write(dir.path().join("projects/broken/project.json"), b"{").ok();
        fs::create_dir_all(dir.path().join("projects/broken")).unwrap();
        fs::write(dir.path().join("projects/broken/project.json"), b"{").unwrap();
        assert_eq!(store.list().unwrap().len(), 2);

        store.delete(&project.id).unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(store.load(&project.id).is_err());
    }
}
