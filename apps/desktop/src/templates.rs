//! Templates on the desktop: the starter templates (the same bundles and
//! `index.json` the web app serves from `/templates/`, built into the app)
//! and your own, project bundles saved in `~/.opencut/templates/` with a
//! small JSON file beside each. Using one imports its bundle as a new
//! project (`editor::import_bundle_as_new`).

use std::fs;
use std::path::{Path, PathBuf};

use editor::{ImportReport, Project, ProjectStore};
use serde::{Deserialize, Serialize};

const STARTER_INDEX: &str = include_str!("../../web/public/templates/index.json");
const STARTER_FILES: &[(&str, &[u8])] = &[
    ("vertical-hook.opencut.zip", include_bytes!("../../web/public/templates/vertical-hook.opencut.zip")),
    ("landscape-title.opencut.zip", include_bytes!("../../web/public/templates/landscape-title.opencut.zip")),
    ("quote-card.opencut.zip", include_bytes!("../../web/public/templates/quote-card.opencut.zip")),
    ("countdown.opencut.zip", include_bytes!("../../web/public/templates/countdown.opencut.zip")),
    ("lower-third.opencut.zip", include_bytes!("../../web/public/templates/lower-third.opencut.zip")),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TemplateEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub fonts: Vec<String>,
    /// The bundle's file name (starter) or path (yours).
    pub file: String,
    #[serde(default)]
    pub mine: bool,
}

/// The templates directory next to the projects root.
pub(crate) fn templates_dir(projects_root: &Path) -> PathBuf {
    projects_root
        .parent()
        .map(|parent| parent.join("templates"))
        .unwrap_or_else(|| projects_root.join("templates"))
}

fn plain(name: &str, suffix: &str) -> bool {
    name.strip_suffix(suffix)
        .is_some_and(|stem| !stem.is_empty() && stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
}

pub(crate) fn starter_templates() -> Vec<TemplateEntry> {
    #[derive(Deserialize)]
    struct Index {
        templates: Vec<TemplateEntry>,
    }
    serde_json::from_str::<Index>(STARTER_INDEX)
        .map(|index| {
            index
                .templates
                .into_iter()
                .filter(|entry| plain(&entry.file, ".opencut.zip") && STARTER_FILES.iter().any(|(file, _)| *file == entry.file))
                .collect()
        })
        .unwrap_or_default()
}

/// Your templates, newest first.
pub(crate) fn my_templates(dir: &Path) -> Vec<TemplateEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, TemplateEntry)> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok()?;
            let mut template: TemplateEntry = serde_json::from_str(&fs::read_to_string(entry.path()).ok()?).ok()?;
            template.mine = true;
            dir.join(&template.file).is_file().then_some((modified, template))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, template)| template).collect()
}

/// Makes a new project from a template (its bundle imported as new).
pub(crate) fn use_template(template: &TemplateEntry, store: &ProjectStore, dir: &Path) -> Result<ImportReport, String> {
    let path = if template.mine {
        dir.join(&template.file)
    } else {
        let (_, bytes) = STARTER_FILES
            .iter()
            .find(|(file, _)| *file == template.file)
            .ok_or_else(|| format!("No starter template {}", template.file))?;
        let starter = dir.join("starter");
        fs::create_dir_all(&starter).map_err(|error| error.to_string())?;
        let path = starter.join(&template.file);
        if fs::read(&path).ok().as_deref() != Some(*bytes) {
            fs::write(&path, bytes).map_err(|error| error.to_string())?;
        }
        path
    };
    editor::import_bundle_as_new(&path, store, Some(&template.name)).map_err(|error| error.to_string())
}

/// Saves `project` (and copies of its media) as one of your templates.
pub(crate) fn save_template(project: &Project, dir: &Path, lookups: &editor::AssetLookups) -> Result<TemplateEntry, String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let id = editor::new_id();
    let file = format!("{id}.opencut.zip");
    editor::export_bundle_with(project, &dir.join(&file), lookups).map_err(|error| error.to_string())?;
    let mut fonts: Vec<String> = project
        .tracks
        .iter()
        .flat_map(|track| track.elements.iter())
        .filter(|element| matches!(element.kind, editor::ElementKind::Text))
        .map(|element| element.params.text("fontFamily", "Arial").into_owned())
        .collect();
    fonts.sort();
    fonts.dedup();
    let template = TemplateEntry {
        id: id.clone(),
        name: project.name.clone(),
        description: String::new(),
        width: project.settings.canvas.width,
        height: project.settings.canvas.height,
        duration: project.duration().to_seconds_f64(),
        fonts,
        file,
        mine: true,
    };
    let json = serde_json::to_string_pretty(&template).map_err(|error| error.to_string())?;
    fs::write(dir.join(format!("{id}.json")), json).map_err(|error| error.to_string())?;
    Ok(template)
}

pub(crate) fn delete_template(template: &TemplateEntry, dir: &Path) -> Result<(), String> {
    if !template.mine {
        return Err("Starter templates cannot be deleted".to_owned());
    }
    fs::remove_file(dir.join(&template.file)).map_err(|error| error.to_string())?;
    fs::remove_file(dir.join(format!("{}.json", template.id))).map_err(|error| error.to_string())
}

/// The template's fonts that are not on disk yet (system fonts excluded).
pub(crate) fn missing_fonts(fonts: &[String], fonts_dir: &Path) -> Vec<String> {
    fonts
        .iter()
        .filter(|family| !editor::SYSTEM_FONTS.contains(&family.as_str()))
        .filter(|family| !fonts_dir.join(crate::fonts::fontsource_id(family)).join("provenance.json").is_file())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_starter_template_is_built_in_and_imports() {
        let starters = starter_templates();
        assert_eq!(starters.len(), STARTER_FILES.len(), "index and built-in files match");
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path().join("projects"));
        let templates = templates_dir(store.root());
        for template in &starters {
            let report = use_template(template, &store, &templates).unwrap();
            assert!(report.skipped.is_empty(), "{}: {:?}", template.id, report.skipped);
            assert_eq!(report.project.name, template.name);
            assert_eq!(report.project.settings.canvas.width, template.width);
            assert!(!report.project.tracks.iter().all(|track| track.elements.is_empty()));
        }
        // Using one twice makes two projects.
        let again = use_template(&starters[0], &store, &templates).unwrap();
        assert_eq!(store.list().unwrap().len(), starters.len() + 1);
        assert_ne!(again.project.id, format!("template-{}", starters[0].id));
    }

    #[test]
    fn saves_lists_uses_and_deletes_your_templates() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectStore::new(dir.path().join("projects"));
        let templates = templates_dir(store.root());
        let mut project = Project::new("My intro");
        editor::apply(
            &mut project,
            &editor::Command::InsertElement {
                element: editor::Element::text("Hello", time::MediaTime::ZERO),
                placement: editor::TrackPlacement::Auto,
            },
        )
        .unwrap();
        let saved = save_template(&project, &templates, &editor::AssetLookups::default()).unwrap();
        assert_eq!(saved.fonts, vec!["Arial".to_owned()]);
        let mine = my_templates(&templates);
        assert_eq!(mine, vec![saved.clone()]);
        let report = use_template(&mine[0], &store, &templates).unwrap();
        assert_eq!(report.project.name, "My intro");
        assert_ne!(report.project.id, project.id);
        delete_template(&mine[0], &templates).unwrap();
        assert!(my_templates(&templates).is_empty());
        assert!(missing_fonts(&["Arial".into(), "Inter".into()], dir.path()).contains(&"Inter".to_owned()));
    }
}
