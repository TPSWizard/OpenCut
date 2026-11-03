use editor::{Project, ProjectStore, ProjectSummary};
use gpui::{
    ClickEvent, Context, EventEmitter, FontWeight, PromptLevel, Window, div, prelude::*, px,
};

use crate::components::{Button, ButtonSize, ButtonVariant, eid};
use crate::format::{date_time, short_duration};
use crate::theme::ActiveTheme;

pub(crate) enum ProjectsEvent {
    Open(Project),
}

pub(crate) struct ProjectsView {
    store: ProjectStore,
    projects: Vec<ProjectSummary>,
    error: Option<String>,
}

impl EventEmitter<ProjectsEvent> for ProjectsView {}

impl ProjectsView {
    pub(crate) fn new(store: ProjectStore) -> Self {
        let mut view = Self {
            store,
            projects: Vec::new(),
            error: None,
        };
        view.refresh();
        view
    }

    pub(crate) fn refresh(&mut self) {
        match self.store.list() {
            Ok(projects) => {
                self.projects = projects;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("Could not list projects: {error}")),
        }
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let count = self.projects.len() + 1;
        match self.store.create(&format!("Untitled {count}")) {
            Ok(project) => {
                self.refresh();
                cx.emit(ProjectsEvent::Open(project));
            }
            Err(error) => self.error = Some(format!("Could not create a project: {error}")),
        }
        cx.notify();
    }

    fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.store.load(id) {
            Ok(project) => cx.emit(ProjectsEvent::Open(project)),
            Err(error) => {
                self.error = Some(format!("Could not open the project: {error}"));
                cx.notify();
            }
        }
    }

    fn delete(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete \"{name}\"?"),
            Some("The project file is removed. Media files stay where they are."),
            &["Delete", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                this.update(cx, |view, cx| {
                    if let Err(error) = view.store.delete(&id) {
                        view.error = Some(format!("Could not delete: {error}"));
                    }
                    view.refresh();
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for ProjectsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = window.theme();
        let colors = theme.colors;

        let rows = self.projects.iter().map(|project| {
            let id = project.id.clone();
            let open_id = id.clone();
            let delete_id = id.clone();
            let name = project.name.clone();
            let delete_name = name.clone();
            div()
                .id(eid("project", &id))
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(12.))
                .py(px(10.))
                .rounded(theme.radius)
                .cursor_pointer()
                .hover(move |style| style.bg(colors.accent))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open(&open_id, cx);
                }))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(name),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(colors.muted_foreground)
                                .child(format!(
                                    "{}  ·  edited {}",
                                    short_duration(project.duration.to_seconds_f64()),
                                    date_time(project.updated_at)
                                )),
                        ),
                )
                .child(
                    Button::new(eid("delete", &id), "Delete")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Small)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.delete(delete_id.clone(), delete_name.clone(), window, cx);
                        })),
                )
        });

        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(640.))
                    .max_w_full()
                    .h_full()
                    .px(px(24.))
                    .py(px(32.))
                    .gap(px(16.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Projects"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(colors.muted_foreground)
                                            .child(self.store.root().display().to_string()),
                                    ),
                            )
                            .child(
                                Button::new("new-project", "New project")
                                    .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                            ),
                    )
                    .when_some(self.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(colors.destructive).child(error))
                    })
                    .child(
                        div()
                            .id("projects-list")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_y_scroll()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(colors.border)
                            .p(px(4.))
                            .when(self.projects.is_empty(), |this| {
                                this.child(
                                    div()
                                        .p(px(24.))
                                        .text_sm()
                                        .text_color(colors.muted_foreground)
                                        .child("No projects yet. Create one to start editing."),
                                )
                            })
                            .children(rows),
                    ),
            )
    }
}
