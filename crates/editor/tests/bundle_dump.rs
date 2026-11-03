//! Writes the project `<root>/<id>/project.json` named by
//! `OPENCUT_BUNDLE_PROJECT=<root>/<id>` as a project bundle to
//! `OPENCUT_BUNDLE_OUTPUT`, or imports the bundle `OPENCUT_BUNDLE_IMPORT`
//! into the store `OPENCUT_BUNDLE_STORE` and prints what came out. A
//! developer aid for checking bundles against the web app; skipped
//! otherwise.

use std::path::PathBuf;

#[test]
fn moves_the_bundle_named_by_the_environment() {
    if let (Ok(path), Ok(output)) = (
        std::env::var("OPENCUT_BUNDLE_PROJECT"),
        std::env::var("OPENCUT_BUNDLE_OUTPUT"),
    ) {
        let path = PathBuf::from(path);
        let id = path.file_name().unwrap().to_string_lossy().into_owned();
        let store = editor::ProjectStore::new(path.parent().unwrap());
        let project = store.load(&id).expect("project loads");
        editor::export_bundle(&project, &PathBuf::from(&output)).expect("bundle written");
        eprintln!("wrote {output}");
    }
    if let (Ok(bundle), Ok(root)) = (
        std::env::var("OPENCUT_BUNDLE_IMPORT"),
        std::env::var("OPENCUT_BUNDLE_STORE"),
    ) {
        let store = editor::ProjectStore::new(root);
        let report = editor::import_bundle(&PathBuf::from(&bundle), &store).expect("bundle imports");
        let project = report.project;
        eprintln!(
            "imported {} ({}): {} media, {} tracks, {} elements; skipped: {:?}",
            project.name,
            project.id,
            project.media.len(),
            project.tracks.len(),
            project.tracks.iter().map(|track| track.elements.len()).sum::<usize>(),
            report.skipped
        );
        for track in &project.tracks {
            for element in &track.elements {
                eprintln!(
                    "  [{:?}/{:?}] {} {} at {:.2}s for {:.2}s, {} keys, {} effects, {} masks",
                    track.lane,
                    track.kind,
                    element.kind.label(),
                    element.name,
                    element.start_time.to_seconds_f64(),
                    element.duration.to_seconds_f64(),
                    element.animations.values().map(|c| c.keys.len()).sum::<usize>(),
                    element.effects.len(),
                    element.masks.len()
                );
            }
        }
    }
}
