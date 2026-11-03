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

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;

        let Some(open) = &self.editor else {
            window.set_window_title("OpenCut");
            return div()
                .size_full()
                .bg(colors.background)
                .text_color(colors.foreground)
                .child(self.projects.clone())
                .into_any_element();
        };

        let state = open.state.read(cx);
        let project = state.project();
        let name = project.name.clone();
        let focus_handle = state.focus_handle.clone();
        let save_label = if state.is_dirty() { "Unsaved changes" } else { "Saved" };
        let export_running = state.export.as_ref().is_some_and(|e| e.finished.is_none());
        window.set_window_title(&format!("{name} – OpenCut"));

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
            .on_action(cx.listener(|this, _: &ExportProject, _, cx| this.open_export(cx)))
            .on_action(cx.listener(|this, _: &CloseProject, _, cx| this.close_project(cx)))
            .child(header)
            .child(div().flex_1().min_h_0().child(open.layout.clone()))
            .when_some(open.export_dialog.clone(), |this, dialog| this.child(dialog))
            .into_any_element()
    }
}
