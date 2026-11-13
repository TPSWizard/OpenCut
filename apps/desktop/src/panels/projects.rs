use editor::{Project, ProjectStore, ProjectSummary};
use gpui::{
    ClickEvent, Context, EventEmitter, FontWeight, PathPromptOptions, PromptLevel, Window, div,
    prelude::*, px,
};

use crate::components::{Button, ButtonSize, ButtonVariant, eid};
use crate::format::{date_time, short_duration};
use crate::theme::ActiveTheme;
use crate::i18n::{t, tf};

pub(crate) enum ProjectsEvent {
    Open(Project),
    /// A project made from a template, with the template's fonts that are
    /// not on disk yet (fetched from Fontsource once it is open).
    OpenFromTemplate(Project, Vec<String>),
}

pub(crate) struct ProjectsView {
    store: ProjectStore,
    projects: Vec<ProjectSummary>,
    error: Option<String>,
    /// Regenerable files in ~/.opencut/processed (proxies, retimed audio).
    cache: (usize, u64),
    /// Your templates, then the starter ones.
    templates: Vec<crate::templates::TemplateEntry>,
}

impl EventEmitter<ProjectsEvent> for ProjectsView {}

impl ProjectsView {
    pub(crate) fn new(store: ProjectStore) -> Self {
        let mut view = Self {
            store,
            projects: Vec::new(),
            error: None,
            cache: (0, 0),
            templates: Vec::new(),
        };
        view.refresh();
        view
    }

    pub(crate) fn refresh(&mut self) {
        self.cache = render::regenerable_cache(&render::default_cache_dir());
        let dir = crate::templates::templates_dir(self.store.root());
        self.templates = crate::templates::my_templates(&dir);
        self.templates.extend(crate::templates::starter_templates());
        match self.store.list() {
            Ok(projects) => {
                self.projects = projects;
                self.error = None;
            }
            Err(error) => self.error = Some(tf("Could not list projects: {error}", &[("error", &error)])),
        }
    }

    /// Sets and saves the interface language, then redraws.
    fn set_language(&mut self, preference: &str, cx: &mut Context<Self>) {
        let mut settings = crate::settings::load();
        settings.locale = preference.to_owned();
        if let Err(message) = crate::settings::save(&settings) {
            self.error = Some(message);
        }
        crate::i18n::init(preference);
        cx.refresh_windows();
        cx.notify();
    }

    fn clear_cache(&mut self, cx: &mut Context<Self>) {
        let (count, bytes) = render::clear_regenerable_cache(&render::default_cache_dir());
        self.cache = render::regenerable_cache(&render::default_cache_dir());
        self.error = Some(tf("Cleared {count} cached file(s), {size}", &[("count", &count), ("size", &crate::format::byte_size(bytes))]));
        cx.notify();
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let count = self.projects.len() + 1;
        match self.store.create(&tf("Untitled {count}", &[("count", &count)])) {
            Ok(project) => {
                self.refresh();
                cx.emit(ProjectsEvent::Open(project));
            }
            Err(error) => self.error = Some(tf("Could not create a project: {error}", &[("error", &error)])),
        }
        cx.notify();
    }

    /// Picks a project bundle (`.opencut.zip`, from this editor or the web
    /// app), copies it into the store and opens it.
    fn import_bundle(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t("Import").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { editor::import_bundle(&path, &store) })
                .await;
            this.update(cx, |view, cx| {
                match result {
                    Ok(report) => {
                        view.refresh();
                        view.error = (!report.skipped.is_empty()).then(|| {
                            tf("Imported without: {items}", &[("items", &report.skipped.join("; "))])
                        });
                        cx.emit(ProjectsEvent::Open(report.project));
                    }
                    Err(error) => view.error = Some(tf("Could not import the bundle: {error}", &[("error", &error)])),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Makes a new project from a template and opens it.
    fn use_template(&mut self, template: crate::templates::TemplateEntry, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let dir = crate::templates::templates_dir(self.store.root());
        let fonts_dir = crate::fonts::fonts_dir(self.store.root());
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::templates::use_template(&template, &store, &dir).map(|report| (report, template.fonts)) })
                .await;
            this.update(cx, |view, cx| {
                match result {
                    Ok((report, fonts)) => {
                        view.refresh();
                        view.error = None;
                        let missing = crate::templates::missing_fonts(&fonts, &fonts_dir);
                        cx.emit(ProjectsEvent::OpenFromTemplate(report.project, missing));
                    }
                    Err(error) => view.error = Some(tf("The template could not be used: {error}", &[("error", &error)])),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn delete_template(&mut self, template: crate::templates::TemplateEntry, cx: &mut Context<Self>) {
        let dir = crate::templates::templates_dir(self.store.root());
        if let Err(error) = crate::templates::delete_template(&template, &dir) {
            self.error = Some(error);
        }
        self.refresh();
        cx.notify();
    }

    fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        match self.store.load(id) {
            Ok(project) => cx.emit(ProjectsEvent::Open(project)),
            Err(error) => {
                self.error = Some(tf("Could not open the project: {error}", &[("error", &error)]));
                cx.notify();
            }
        }
    }

    fn delete(&mut self, id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &tf("Delete {name}?", &[("name", &format!("\"{name}\""))]),
            Some(t("The project file is removed. Media files stay where they are.")),
            &[t("Delete"), t("Cancel")],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                this.update(cx, |view, cx| {
                    if let Err(error) = view.store.delete(&id) {
                        view.error = Some(tf("Could not delete: {error}", &[("error", &error)]));
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
                                .child(tf(
                                    "{duration}  ·  edited {date}",
                                    &[
                                        ("duration", &short_duration(project.duration.to_seconds_f64())),
                                        ("date", &date_time(project.updated_at)),
                                    ],
                                )),
                        ),
                )
                .child(
                    Button::new(eid("delete", &id), t("Delete"))
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
                                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(t("Projects")))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(colors.muted_foreground)
                                            .child(self.store.root().display().to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        Button::new("import-bundle", t("Import bundle…"))
                                            .variant(ButtonVariant::Outline)
                                            .on_click(cx.listener(|this, _, _, cx| this.import_bundle(cx))),
                                    )
                                    .child(
                                        Button::new("new-project", t("New project"))
                                            .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                                    ),
                            ),
                    )
                    .when_some(self.error.clone(), |this, error| {
                        this.child(div().text_xs().text_color(colors.destructive).child(error))
                    })
                    .when(!self.templates.is_empty(), |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(t("Start from a template")))
                                .child(div().flex().flex_wrap().gap(px(6.)).children(self.templates.iter().map(|template| {
                                    let entry = template.clone();
                                    let label = format!(
                                        "{}  ·  {}×{}  ·  {}",
                                        template.name,
                                        template.width,
                                        template.height,
                                        short_duration(template.duration)
                                    );
                                    let mut row = div().flex().items_center().gap(px(2.)).child(
                                        Button::new(eid("template", &template.id), label)
                                            .variant(if template.mine { ButtonVariant::Secondary } else { ButtonVariant::Outline })
                                            .size(ButtonSize::Small)
                                            .on_click(cx.listener(move |this, _, _, cx| this.use_template(entry.clone(), cx))),
                                    );
                                    if template.mine {
                                        let entry = template.clone();
                                        row = row.child(
                                            Button::new(eid("template-delete", &template.id), "×")
                                                .variant(ButtonVariant::Ghost)
                                                .size(ButtonSize::XSmall)
                                                .on_click(cx.listener(move |this, _, _, cx| this.delete_template(entry.clone(), cx))),
                                        );
                                    }
                                    row
                                }))),
                        )
                    })
                    .child(
                        div()
                            .id("projects-list")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
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
                                        .child(t("No projects yet. Create one to start editing.")),
                                )
                            })
                            .children(rows),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_xs()
                            .text_color(colors.muted_foreground)
                            .child(tf(
                                "Cache you can clear (proxies, retimed audio): {count} file(s), {size}. They are made again when needed.",
                                &[("count", &self.cache.0), ("size", &crate::format::byte_size(self.cache.1))],
                            ))
                            .child(div().flex_1())
                            .child(t("Language"))
                            .children(
                                std::iter::once(("auto", t("Auto")))
                                    .chain(crate::i18n::Locale::ALL.iter().map(|locale| (locale.id(), locale.label())))
                                    .map(|(id, label)| {
                                        let current = crate::settings::load().locale;
                                        Button::new(eid("language", id), label)
                                            .variant(if current == id { ButtonVariant::Secondary } else { ButtonVariant::Ghost })
                                            .size(ButtonSize::XSmall)
                                            .on_click(cx.listener(move |this, _, _, cx| this.set_language(id, cx)))
                                    }),
                            )
                            .when(self.cache.0 > 0, |row| {
                                row.child(
                                    Button::new("clear-cache", t("Clear"))
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::XSmall)
                                        .on_click(cx.listener(|this, _, _, cx| this.clear_cache(cx))),
                                )
                            }),
                    ),
            )
    }
}
