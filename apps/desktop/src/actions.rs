use gpui::{App, KeyBinding, actions};

actions!(
    opencut,
    [
        TogglePlayback,
        StepBack,
        StepForward,
        GoToStart,
        GoToEnd,
        PreviousKeyframe,
        NextKeyframe,
        Undo,
        Redo,
        Split,
        DeleteSelection,
        DuplicateSelection,
        SelectAll,
        ClearSelection,
        DrawMaskPath,
        FinishPenPath,
        ZoomIn,
        ZoomOut,
        ImportMedia,
        AddText,
        SaveProject,
        ExportProject,
        CloseProject,
        Quit,
        MarkIn,
        MarkOut,
        ClearMarks,
        GoToMarkIn,
        GoToMarkOut,
        NextEdit,
        PreviousEdit,
        RippleDelete,
        DeleteMarkedRange,
    ]
);

/// A shortcut preset: which keys split, delete, ripple-delete and clear
/// the marks. The rest of the map is the same in every preset.
pub(crate) struct KeymapPreset {
    pub id: &'static str,
    pub label: &'static str,
    split: &'static [&'static str],
    delete: &'static [&'static str],
    ripple_delete: &'static [&'static str],
    clear_marks: &'static [&'static str],
}

pub(crate) const KEYMAP_PRESETS: &[KeymapPreset] = &[
    KeymapPreset {
        id: "opencut",
        label: "ConzaCut",
        split: &["s", "{primary}-b"],
        delete: &["delete", "backspace"],
        ripple_delete: &["shift-delete", "shift-backspace"],
        clear_marks: &["{primary}-shift-x"],
    },
    KeymapPreset {
        id: "premiere",
        label: "Premiere Pro",
        split: &["{primary}-k", "{primary}-b"],
        delete: &["delete", "backspace"],
        ripple_delete: &["shift-delete", "shift-backspace"],
        clear_marks: &["{primary}-shift-x"],
    },
    KeymapPreset {
        id: "final-cut",
        label: "Final Cut Pro",
        split: &["{primary}-b"],
        delete: &["shift-delete", "shift-backspace"],
        ripple_delete: &["delete", "backspace"],
        clear_marks: &["alt-x"],
    },
    KeymapPreset {
        id: "resolve",
        label: "DaVinci Resolve",
        split: &["{primary}-b"],
        delete: &["delete", "backspace"],
        ripple_delete: &["shift-delete", "shift-backspace"],
        clear_marks: &["alt-x"],
    },
];

/// The preset to use: `OPENCUT_KEYMAP`, else `"preset"` in
/// `~/.opencut/keymap.json`, else the editor's own.
pub(crate) fn chosen_preset() -> &'static KeymapPreset {
    let wanted = std::env::var("OPENCUT_KEYMAP").ok().or_else(|| {
        let path = editor::ProjectStore::default_root().parent()?.join("keymap.json");
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value.get("preset")?.as_str().map(str::to_owned)
    });
    wanted
        .and_then(|id| KEYMAP_PRESETS.iter().find(|preset| preset.id == id))
        .unwrap_or(&KEYMAP_PRESETS[0])
}

/// The editor's shortcuts. They apply while an editor is open and no text
/// field has focus.
pub(crate) fn bind_keys(cx: &mut App) {
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let editor = Some("Editor && !TextInput");
    let anywhere = Some("Editor");
    let preset = chosen_preset();
    let keys = |list: &[&str]| -> Vec<String> { list.iter().map(|key| key.replace("{primary}", primary)).collect() };
    let mut preset_bindings = Vec::new();
    for key in keys(preset.split) {
        preset_bindings.push(KeyBinding::new(&key, Split, editor));
    }
    for key in keys(preset.delete) {
        preset_bindings.push(KeyBinding::new(&key, DeleteSelection, editor));
    }
    for key in keys(preset.ripple_delete) {
        preset_bindings.push(KeyBinding::new(&key, RippleDelete, editor));
    }
    for key in keys(preset.clear_marks) {
        preset_bindings.push(KeyBinding::new(&key, ClearMarks, editor));
    }
    cx.bind_keys(preset_bindings);
    cx.bind_keys([
        KeyBinding::new("space", TogglePlayback, editor),
        KeyBinding::new("k", TogglePlayback, editor),
        KeyBinding::new("left", StepBack, editor),
        KeyBinding::new("right", StepForward, editor),
        KeyBinding::new("home", GoToStart, editor),
        KeyBinding::new("end", GoToEnd, editor),
        KeyBinding::new("shift-left", PreviousKeyframe, editor),
        KeyBinding::new("shift-right", NextKeyframe, editor),
        KeyBinding::new(&format!("{primary}-z"), Undo, anywhere),
        KeyBinding::new(&format!("{primary}-shift-z"), Redo, anywhere),
        KeyBinding::new(&format!("{primary}-y"), Redo, anywhere),
        KeyBinding::new("i", MarkIn, editor),
        KeyBinding::new("o", MarkOut, editor),
        KeyBinding::new("shift-i", GoToMarkIn, editor),
        KeyBinding::new("shift-o", GoToMarkOut, editor),
        KeyBinding::new("down", NextEdit, editor),
        KeyBinding::new("up", PreviousEdit, editor),
        KeyBinding::new("alt-delete", DeleteMarkedRange, editor),
        KeyBinding::new(&format!("{primary}-d"), DuplicateSelection, editor),
        KeyBinding::new(&format!("{primary}-a"), SelectAll, editor),
        KeyBinding::new("escape", ClearSelection, anywhere),
        KeyBinding::new("p", DrawMaskPath, editor),
        KeyBinding::new("enter", FinishPenPath, editor),
        KeyBinding::new(&format!("{primary}-="), ZoomIn, anywhere),
        KeyBinding::new(&format!("{primary}--"), ZoomOut, anywhere),
        KeyBinding::new(&format!("{primary}-i"), ImportMedia, anywhere),
        KeyBinding::new("t", AddText, editor),
        KeyBinding::new(&format!("{primary}-s"), SaveProject, anywhere),
        KeyBinding::new(&format!("{primary}-e"), ExportProject, anywhere),
        KeyBinding::new(&format!("{primary}-w"), CloseProject, anywhere),
        KeyBinding::new(&format!("{primary}-q"), Quit, None),
    ]);
}
