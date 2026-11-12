use std::sync::Arc;

use editor::{Project, ProjectStore};
use gpui::{
    AppContext, Context, Entity, FontWeight, PathPromptOptions, Window, div, prelude::*, px,
};
use media::Ffmpeg;

use crate::actions::*;
use crate::components::{Button, ButtonSize, ButtonVariant, Orientation, ResizablePanelGroup};
use crate::panels::{
    ExportDialog, ExportEvent, InspectorPanel, MediaPanel, PreviewPanel, ProjectsEvent,
    ProjectsView, TimelinePanel,
};
use crate::state::EditorState;
use crate::theme::ActiveTheme;

/// Everything that exists while a project is open.
struct OpenEditor {
    state: Entity<EditorState>,
    layout: Entity<ResizablePanelGroup>,
    export_dialog: Option<Entity<ExportDialog>>,
    /// The editor takes keyboard focus on its first render, so shortcuts
    /// work before anything has been clicked.
    focus_pending: bool,
}

pub(crate) struct Shell {
    store: ProjectStore,
    ffmpeg: Option<Arc<Ffmpeg>>,
    projects: Entity<ProjectsView>,
    editor: Option<OpenEditor>,
}

impl Shell {
    pub(crate) fn new(store: ProjectStore, ffmpeg: Option<Arc<Ffmpeg>>, cx: &mut Context<Self>) -> Self {
        let projects = cx.new(|_| ProjectsView::new(store.clone()));
        cx.subscribe(&projects, |this, _, event: &ProjectsEvent, cx| match event {
            ProjectsEvent::Open(project) => this.open_project(project.clone(), cx),
        })
        .detach();
        Self {
            store,
            ffmpeg,
            projects,
            editor: None,
        }
    }

    fn open_project(&mut self, project: Project, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let ffmpeg = self.ffmpeg.clone();
        let state = cx.new(|cx| EditorState::new(project, store, ffmpeg, cx));
        cx.observe(&state, |_, _, cx| cx.notify()).detach();

        let media = cx.new(|cx| MediaPanel::new(state.clone(), cx));
        let preview = cx.new(|cx| PreviewPanel::new(state.clone(), cx));
        let timeline = cx.new(|cx| TimelinePanel::new(state.clone(), cx));
        // The inspector needs a window for its text fields; it is created
        // on first render (see `ensure_inspector`).
        let inspector_slot: Entity<InspectorSlot> = cx.new(|_| InspectorSlot {
            state: state.clone(),
            inspector: None,
        });
        let right = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Horizontal, preview, inspector_slot)
                .initial_fraction(0.68)
                .minimum_fraction(0.2)
        });
        let top = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Horizontal, media, right)
                .initial_fraction(0.22)
                .minimum_fraction(0.12)
        });
        let layout = cx.new(|_| {
            ResizablePanelGroup::new(Orientation::Vertical, top, timeline)
                .initial_fraction(0.6)
                .minimum_fraction(0.2)
        });
        self.editor = Some(OpenEditor {
            state,
            layout,
            export_dialog: None,
            focus_pending: true,
        });
        cx.notify();
    }

    fn close_project(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = self.editor.take() {
            open.state.update(cx, |state, cx| state.close(cx));
        }
        self.projects.update(cx, |projects, cx| {
            projects.refresh();
            cx.notify();
        });
        cx.notify();
    }

    fn open_export(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &mut self.editor else { return };
        if open.export_dialog.is_some() {
            return;
        }
        let dialog = cx.new(|cx| ExportDialog::new(open.state.clone(), cx));
        cx.subscribe(&dialog, |this, _, event: &ExportEvent, cx| match event {
            ExportEvent::Close => {
                if let Some(open) = &mut this.editor {
                    open.export_dialog = None;
                    cx.notify();
                }
            }
        })
        .detach();
        open.export_dialog = Some(dialog);
        cx.notify();
    }

    /// Saves the project and writes it with copies of its media as a
    /// project bundle (`.opencut.zip`), which the web app can import.
    fn export_bundle(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.editor else {
            return;
        };
        let state = open.state.clone();
        let name = state.read(cx).project().name.clone();
        let suggested = format!("{}.opencut.zip", bundle_file_name(&name));
        let directory = std::env::current_dir().unwrap_or_default();
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(mut path))) = receiver.await else {
                return;
            };
            if path.extension().is_none() {
                path.set_extension("zip");
            }
            let Ok(project) = state.update(cx, |state, cx| {
                state.save_now(cx);
                state.project().clone()
            }) else {
                return;
            };
            let shown = path.display().to_string();
            let result = cx
                .background_executor()
                .spawn(async move { editor::export_bundle(&project, &path) })
                .await;
            state
                .update(cx, |state, cx| match result {
                    Ok(()) => state.set_status(format!("Saved bundle {shown}"), cx),
                    Err(error) => state.set_status(format!("Bundle not saved: {error}"), cx),
                })
                .ok();
        })
        .detach();
    }

    /// Writes the captions (the text elements) as a subtitle file; the
    /// extension the user picks (`.srt`, `.vtt`, `.ass`) chooses the format.
    fn export_captions(&mut self, cx: &mut Context<Self>) {
        let Some(open) = &self.editor else {
            return;
        };
        let state = open.state.clone();
        let name = state.read(cx).project().name.clone();
        let suggested = format!("{}.srt", bundle_file_name(&name));
        let directory = std::env::current_dir().unwrap_or_default();
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(mut path))) = receiver.await else {
                return;
            };
            if editor::SubtitleFormat::from_path(&path).is_none() {
                path.set_extension("srt");
            }
            let Ok(project) = state.update(cx, |state, _| state.project().clone()) else {
                return;
            };
            let shown = path.display().to_string();
            let result = cx
                .background_executor()
                .spawn(async move { editor::export_subtitles(&project, &path, None) })
                .await;
            state
                .update(cx, |state, cx| match result {
                    Ok(count) => state.set_status(format!("Saved {count} caption(s) to {shown}"), cx),
                    Err(error) => state.set_status(format!("Captions not saved: {error}"), cx),
                })
                .ok();
        })
        .detach();
    }

    fn with_state(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorState, &mut Context<EditorState>)) {
        if let Some(open) = &self.editor {
            open.state.update(cx, f);
        }
    }
}

/// Holds the inspector once a window exists to build its inputs in.
struct InspectorSlot {
    state: Entity<EditorState>,
    inspector: Option<Entity<InspectorPanel>>,
}

impl Render for InspectorSlot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.inspector.is_none() {
            let state = self.state.clone();
            self.inspector = Some(cx.new(|cx| InspectorPanel::new(state, window, cx)));
        }
        div()
            .size_full()
            .child(self.inspector.clone().expect("created above"))
    }
}

/// A file name for the project: no path separators or characters Windows
/// refuses, and never empty.
fn bundle_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() { "project".to_owned() } else { trimmed.to_owned() }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;

        let Some(open) = &mut self.editor else {
            window.set_window_title("ConzaCut");
            return div()
                .size_full()
                .bg(colors.background)
                .text_color(colors.foreground)
                .child(self.projects.clone())
                .into_any_element();
        };
        if open.focus_pending {
            open.focus_pending = false;
            let handle = open.state.read(cx).focus_handle.clone();
            window.focus(&handle);
        }
        let open = &*open;

        let state = open.state.read(cx);
        let project = state.project();
        let name = project.name.clone();
        let focus_handle = state.focus_handle.clone();
        let save_label = if state.is_dirty() { "Unsaved changes" } else { "Saved" };
        let export_running = state.export.as_ref().is_some_and(|e| e.finished.is_none());
        window.set_window_title(&format!("{name} – ConzaCut"));

        let state_entity = open.state.clone();
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(40.))
            .px(px(8.))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.card)
            .child(
                Button::new("back", "← Projects")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.close_project(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(name),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child(save_label),
            )
            .child({
                let state = state_entity.clone();
                Button::new("import-header", "Import")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Small)
                    .on_click(move |_, _, cx| MediaPanel::import(&state, cx))
            })
            .child(
                Button::new("export-captions", "Captions…")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.export_captions(cx))),
            )
            .child(
                Button::new("export-bundle", "Save bundle…")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.export_bundle(cx))),
            )
            .child(
                Button::new("export-header", if export_running { "Exporting…" } else { "Export" })
                    .size(ButtonSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.open_export(cx))),
            );

        div()
            .id("editor-root")
            .key_context("Editor")
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.background)
            .text_color(colors.foreground)
            .when(crate::state::trace_input(), |this| {
                // Logs every wheel event the window receives, before any element handles it.
                this.child(
                    gpui::canvas(
                        |_, _, _| (),
                        |_, _, window, _| {
                            window.on_mouse_event(|event: &gpui::ScrollWheelEvent, phase, window, _| {
                                if phase == gpui::DispatchPhase::Capture {
                                    eprintln!(
                                        "trace: window wheel delta={:?} position={:?} modifiers={:?} mouse={:?}",
                                        event.delta,
                                        event.position,
                                        event.modifiers,
                                        window.mouse_position()
                                    );
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| {
                this.with_state(cx, |s, cx| s.toggle_playback(cx))
            }))
            .on_action(cx.listener(|this, _: &StepBack, _, cx| {
                this.with_state(cx, |s, cx| s.step_frames(-1, cx))
            }))
            .on_action(cx.listener(|this, _: &StepForward, _, cx| {
                this.with_state(cx, |s, cx| s.step_frames(1, cx))
            }))
            .on_action(cx.listener(|this, _: &PreviousKeyframe, _, cx| {
                this.with_state(cx, |s, cx| s.jump_to_keyframe(false, cx))
            }))
            .on_action(cx.listener(|this, _: &NextKeyframe, _, cx| {
                this.with_state(cx, |s, cx| s.jump_to_keyframe(true, cx))
            }))
            .on_action(cx.listener(|this, _: &GoToStart, _, cx| {
                this.with_state(cx, |s, cx| s.seek(time::MediaTime::ZERO, cx))
            }))
            .on_action(cx.listener(|this, _: &GoToEnd, _, cx| {
                this.with_state(cx, |s, cx| {
                    let end = s.duration();
                    s.seek(end, cx)
                })
            }))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.with_state(cx, |s, cx| s.undo(cx))))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.with_state(cx, |s, cx| s.redo(cx))))
            .on_action(cx.listener(|this, _: &Split, _, cx| {
                this.with_state(cx, |s, cx| s.split_at_playhead(cx))
            }))
            .on_action(cx.listener(|this, _: &DeleteSelection, _, cx| {
                this.with_state(cx, |s, cx| s.delete_selection(cx))
            }))
            .on_action(cx.listener(|this, _: &DuplicateSelection, _, cx| {
                this.with_state(cx, |s, cx| s.duplicate_selection(cx))
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.with_state(cx, |s, cx| s.select_all(cx))
            }))
            .on_action(cx.listener(|this, _: &DrawMaskPath, _, cx| {
                this.with_state(cx, |s, cx| s.start_pen(cx))
            }))
            .on_action(cx.listener(|this, _: &FinishPenPath, _, cx| {
                this.with_state(cx, |s, cx| s.finish_pen(cx))
            }))
            .on_action(cx.listener(|this, _: &ClearSelection, _, cx| {
                this.with_state(cx, |s, cx| s.clear_selection(cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                this.with_state(cx, |s, cx| s.zoom_by(1.4, None, cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                this.with_state(cx, |s, cx| s.zoom_by(1.0 / 1.4, None, cx))
            }))
            .on_action(cx.listener(|this, _: &AddText, _, cx| {
                this.with_state(cx, |s, cx| s.add_text(cx))
            }))
            .on_action(cx.listener(|this, _: &SaveProject, _, cx| {
                this.with_state(cx, |s, cx| s.save_now(cx))
            }))
            .on_action(cx.listener(|this, _: &ImportMedia, _, cx| {
                if let Some(open) = &this.editor {
                    let state = open.state.clone();
                    let receiver = cx.prompt_for_paths(PathPromptOptions {
                        files: true,
                        directories: false,
                        multiple: true,
                        prompt: Some("Import".into()),
                    });
                    cx.spawn(async move |_, cx| {
                        if let Ok(Ok(Some(paths))) = receiver.await {
                            state
                                .update(cx, |state, cx| state.import_paths(paths, cx))
                                .ok();
                        }
                    })
                    .detach();
                }
            }))
            .on_action(cx.listener(|this, _: &MarkIn, _, cx| this.with_state(cx, |s, cx| s.set_mark_in(cx))))
            .on_action(cx.listener(|this, _: &MarkOut, _, cx| this.with_state(cx, |s, cx| s.set_mark_out(cx))))
            .on_action(cx.listener(|this, _: &ClearMarks, _, cx| this.with_state(cx, |s, cx| s.clear_marks(cx))))
            .on_action(cx.listener(|this, _: &GoToMarkIn, _, cx| this.with_state(cx, |s, cx| s.goto_mark(false, cx))))
            .on_action(cx.listener(|this, _: &GoToMarkOut, _, cx| this.with_state(cx, |s, cx| s.goto_mark(true, cx))))
            .on_action(cx.listener(|this, _: &NextEdit, _, cx| this.with_state(cx, |s, cx| s.jump_to_edit(true, cx))))
            .on_action(cx.listener(|this, _: &PreviousEdit, _, cx| this.with_state(cx, |s, cx| s.jump_to_edit(false, cx))))
            .on_action(cx.listener(|this, _: &RippleDelete, _, cx| this.with_state(cx, |s, cx| s.ripple_delete_selection(cx))))
            .on_action(cx.listener(|this, _: &DeleteMarkedRange, _, cx| this.with_state(cx, |s, cx| s.delete_marked_range(cx))))
            .on_action(cx.listener(|this, _: &ExportProject, _, cx| this.open_export(cx)))
            .on_action(cx.listener(|this, _: &CloseProject, _, cx| this.close_project(cx)))
            .child(header)
            .child(div().flex_1().min_h_0().child(open.layout.clone()))
            .when_some(open.export_dialog.clone(), |this, dialog| this.child(dialog))
            .into_any_element()
    }
}
