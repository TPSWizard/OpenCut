//! A project plus its undo and redo stacks.

use crate::commands::{Command, Outcome, apply};
use crate::document::Project;
use crate::error::EditorError;

const HISTORY_LIMIT: usize = 200;

#[derive(Clone, Debug)]
pub struct Editor {
    project: Project,
    undo_stack: Vec<Project>,
    redo_stack: Vec<Project>,
    /// Bumped on every change, so observers can tell whether anything moved.
    revision: u64,
}

impl Editor {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            revision: 0,
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Applies a command, recording the previous state for undo.
    pub fn execute(&mut self, command: &Command) -> Result<Outcome, EditorError> {
        let snapshot = self.project.clone();
        let outcome = apply(&mut self.project, command)?;
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > HISTORY_LIMIT {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.revision += 1;
        Ok(outcome)
    }

    /// Applies a command without recording it. Used for things that are not
    /// edits: the playhead position, for example.
    pub fn apply_transient(&mut self, update: impl FnOnce(&mut Project)) {
        update(&mut self.project);
        self.revision += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self) -> Result<(), EditorError> {
        let previous = self.undo_stack.pop().ok_or(EditorError::NothingTo("undo"))?;
        let current = std::mem::replace(&mut self.project, previous);
        self.project.playhead = current.playhead;
        self.redo_stack.push(current);
        self.revision += 1;
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), EditorError> {
        let next = self.redo_stack.pop().ok_or(EditorError::NothingTo("redo"))?;
        let current = std::mem::replace(&mut self.project, next);
        self.project.playhead = current.playhead;
        self.undo_stack.push(current);
        self.revision += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::TrackPlacement;
    use crate::document::Element;
    use time::MediaTime;

    #[test]
    fn undo_and_redo_restore_snapshots() {
        let mut editor = Editor::new(Project::new("History"));
        assert!(!editor.can_undo());
        editor
            .execute(&Command::InsertElement {
                element: Element::text("One", MediaTime::ZERO),
                placement: TrackPlacement::Auto,
            })
            .unwrap();
        editor
            .execute(&Command::RenameProject("Renamed".to_owned()))
            .unwrap();
        assert_eq!(editor.project().name, "Renamed");
        editor.undo().unwrap();
        assert_eq!(editor.project().name, "History");
        assert_eq!(editor.project().elements().count(), 1);
        editor.undo().unwrap();
        assert_eq!(editor.project().elements().count(), 0);
        assert_eq!(editor.undo(), Err(EditorError::NothingTo("undo")));
        editor.redo().unwrap();
        editor.redo().unwrap();
        assert_eq!(editor.project().name, "Renamed");
        assert!(!editor.can_redo());
        // A new edit clears the redo stack.
        editor.undo().unwrap();
        editor
            .execute(&Command::RenameProject("Other".to_owned()))
            .unwrap();
        assert!(!editor.can_redo());
    }

    #[test]
    fn failed_commands_leave_no_history() {
        let mut editor = Editor::new(Project::new("History"));
        assert!(
            editor
                .execute(&Command::DeleteElements(vec!["missing".to_owned()]))
                .is_err()
        );
        assert!(!editor.can_undo());
        assert_eq!(editor.revision(), 0);
    }
}
