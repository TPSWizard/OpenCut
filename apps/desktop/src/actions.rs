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
        ZoomIn,
        ZoomOut,
        ImportMedia,
        AddText,
        SaveProject,
        ExportProject,
        CloseProject,
        Quit,
    ]
);

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
        KeyBinding::new("s", Split, editor),
        KeyBinding::new("delete", DeleteSelection, editor),
        KeyBinding::new("backspace", DeleteSelection, editor),
        KeyBinding::new(&format!("{primary}-d"), DuplicateSelection, editor),
        KeyBinding::new(&format!("{primary}-a"), SelectAll, editor),
        KeyBinding::new("escape", ClearSelection, anywhere),
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
