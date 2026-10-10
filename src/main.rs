#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_assets;
mod library;
mod local_metadata;
mod online;
mod player;
mod theme;
mod ui;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

fn main() {
    gpui_kit::application()
        .with_assets(app_assets::AppAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            theme::apply(false, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.bind_keys([
                KeyBinding::new("ctrl-o", ui::ImportMusic, Some("Music")),
                KeyBinding::new("alt-left", ui::NavigateBack, Some("Music")),
                KeyBinding::new("alt-right", ui::NavigateForward, Some("Music")),
                KeyBinding::new("escape", ui::CloseLyrics, Some("Music")),
                KeyBinding::new("ctrl-f", ui::SearchMusic, Some("Music")),
                KeyBinding::new("space", ui::TogglePlayback, Some("Music && !Input")),
                KeyBinding::new("ctrl-right", ui::NextTrack, Some("Music")),
                KeyBinding::new("ctrl-left", ui::PreviousTrack, Some("Music")),
            ]);

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1180.), px(780.)),
                    cx,
                ))),
                window_min_size: Some(size(px(920.), px(620.))),
                ..TitleBar::window_options()
            };

            let (handle, view) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::MigApp::new(window, cx))
            })
            .expect("无法打开音乐窗口");

            #[cfg(feature = "ui-verify")]
            if let Some(directory) = std::env::var_os("MIG_VERIFY_UI") {
                ui::verify::run(handle, view, directory.into(), cx);
            }
            #[cfg(not(feature = "ui-verify"))]
            let _ = (handle, view);
            cx.activate(true);
        });
}

#[cfg(all(test, feature = "ui-verify"))]
#[::core::prelude::v1::test]
#[ignore = "opens a native window; set MIG_VERIFY_UI and isolated MIG_MUSIC_DATA_DIR"]
fn native_queue_menu() {
    assert!(std::env::var_os("MIG_VERIFY_QUEUE_MENU").is_some());
    assert!(std::env::var_os("MIG_MUSIC_DATA_DIR").is_some());
    let directory = std::env::var_os("MIG_VERIFY_UI").expect("set MIG_VERIFY_UI");
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).expect("queue verification directory");
    let report_path = directory.join("result.txt");
    std::fs::write(&report_path, "RUNNING\n").expect("reset queue verification report");
    main();
    let report = std::fs::read_to_string(report_path).expect("queue verification report");
    assert!(report.starts_with("PASS:"), "{report}");
}
