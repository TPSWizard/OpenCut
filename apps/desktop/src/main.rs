#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use editor::ProjectStore;
use gpui::{
    App, AppContext, Application, Bounds, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px, size,
};
use media::Ffmpeg;

mod actions;
mod assets;
mod audio;
mod beats;
mod ducking;
mod fonts;
mod components;
mod format;
mod i18n;
mod images;
mod loudness;
mod luts;
mod panels;
mod reframe;
mod voiceover;
mod render_thread;
mod settings;
mod shell;
mod state;
mod stickers;
mod stock;
mod templates;
mod transcribe;
mod theme;

use shell::Shell;

fn main() {
    #[cfg(target_os = "linux")]
    {
        let is_wsl = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .is_ok_and(|release| release.to_ascii_lowercase().contains("microsoft"));
        let x11_configured =
            std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
        let wayland_configured =
            std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty());

        if is_wsl && x11_configured && wayland_configured {
            // GPUI 0.2.2 requires xdg_wm_base v2+, while current WSLg advertises v1 and panics.
            // Safety: this is the first statement in `main`, before GPUI or anything
            // else has spawned a thread. No other thread exists yet, so keep this
            // block first if anything is added above it.
            unsafe {
                std::env::remove_var("WAYLAND_DISPLAY");
            }
        }
    }

    // Projects live in ~/.opencut/projects unless OPENCUT_PROJECTS says otherwise.
    let store = std::env::var_os("OPENCUT_PROJECTS")
        .map(ProjectStore::new)
        .unwrap_or_else(|| ProjectStore::new(ProjectStore::default_root()));
    let ffmpeg = match Ffmpeg::locate() {
        Ok(ffmpeg) => {
            eprintln!("opencut: using {} ({})", ffmpeg.ffmpeg_path().display(), ffmpeg.version());
            Some(ffmpeg)
        }
        Err(error) => {
            eprintln!("opencut: {error}");
            None
        }
    };

    Application::new().run(move |cx: &mut App| {
        i18n::init(&settings::load().locale);
        actions::bind_keys(cx);
        components::bind_text_input_keys(cx);
        cx.on_action(|_: &actions::Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let store = store.clone();
        let ffmpeg = ffmpeg.clone();
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("ConzaCut")),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Maximized(bounds)),
                focus: true,
                ..Default::default()
            },
            move |window, cx| {
                cx.new(|cx| {
                    cx.observe_window_appearance(window, |_, window, _| {
                        window.refresh();
                    })
                    .detach();
                    Shell::new(store, ffmpeg, cx)
                })
            },
        )
        .expect("failed to open the main window");
        cx.activate(true);
    });
}
