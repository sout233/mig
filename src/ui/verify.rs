//! Run window checks through GPUI's test input API
use super::{MigApp, Page};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AsyncApp, Entity, ScrollDelta, SharedString, point, px, size,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn run(handle: AnyWindowHandle, view: Entity<MigApp>, directory: PathBuf, cx: &mut App) {
    fs::create_dir_all(&directory).expect("create verification directory");
    cx.spawn(async move |cx| {
        let result = verify(handle, &view, &directory, cx).await;
        let report = match result {
            Ok(()) => "PASS: native button input, audio playback, pause, seek, next/previous, favorites, search, queue, theme, minimum window, zoom, lyrics, missing-file recovery and online/source pages. Online playback checked when MIG_VERIFY_ONLINE is enabled.\n",
            Err(ref error) => error,
        };

        let _ = fs::write(directory.join("result.txt"), report.as_bytes());
        cx.update(|cx| cx.quit());
    }).detach();
}

async fn wait_for(
    handle: AnyWindowHandle,
    view: &Entity<MigApp>,
    cx: &mut AsyncApp,
    predicate: impl Fn(&MigApp) -> bool,
) -> Result<(), String> {
    for _ in 0..300 {
        if handle
            .update(cx, |_, _, cx| predicate(view.read(cx)))
            .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
    }
    handle.update(cx, |_, _, cx| {
        let app = view.read(cx);
        Err(format!("FAIL: timed out waiting for application state; page={:?}; query={:?}; searching={}; opening={}; playing={}; visible={}; error={:?}",
            app.page, app.query, app.searching, app.opening, app.playing, app.visible.len(), app.error))
    }).map_err(|error| error.to_string())?
}

fn screenshot(handle: AnyWindowHandle, path: &Path, cx: &mut AsyncApp) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window
                .render_to_image()
                .map_err(|error| error.to_string())?
                .save(path)
                .map_err(|error| error.to_string())
        })
        .map_err(|error| error.to_string())?
}

fn click(
    handle: AnyWindowHandle,
    id: impl Into<gpui_kit::ElementId>,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.click(id, cx);
        })
        .map_err(|error| error.to_string())
}

async fn hover_screenshot(
    handle: AnyWindowHandle,
    id: &'static str,
    path: &Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.hover(id, cx);
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    screenshot(handle, path, cx)
}

fn local_audio_fixtures(directory: &Path) -> Result<Vec<PathBuf>, String> {
    // These audio fixtures are just for the checks; the app does not ship demo songs
    let rate = 8000u32;
    let bytes = rate * 32 * 2;
    let mut wave = Vec::with_capacity(bytes as usize + 44);
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(bytes + 36).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt ");
    wave.extend_from_slice(&16u32.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&1u16.to_le_bytes());
    wave.extend_from_slice(&rate.to_le_bytes());
    wave.extend_from_slice(&(rate * 2).to_le_bytes());
    wave.extend_from_slice(&2u16.to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(b"data");
    wave.extend_from_slice(&bytes.to_le_bytes());
    wave.resize(bytes as usize + 44, 0);
    let mut paths = Vec::new();
    for title in ["晨光", "雨后", "晚风"] {
        let path = directory.join(format!("{title}.wav"));
        fs::write(&path, &wave).map_err(|error| error.to_string())?;
        paths.push(path);
    }
    Ok(paths)
}

async fn verify_expanded_layout(
    handle: AnyWindowHandle,
    view: &Entity<MigApp>,
    directory: &Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let original = handle
        .update(cx, |_, _, cx| {
            let view = view.read(cx);
            (view.lyrics.clone(), view.lyrics_bilingual)
        })
        .map_err(|error| error.to_string())?;

    for bilingual in [false, true] {
        handle
            .update(cx, |_, _, cx| {
                view.update(cx, |view, cx| {
                    view.lyrics = if bilingual {
                        crate::library::parse_translated_lrc(
                            "[00:01]Original line\n[00:02]Another line",
                            "[00:01]第一行翻译\n[00:02]第二行翻译",
                        )
                    } else {
                        Vec::new()
                    };
                    view.lyrics_bilingual = bilingual;
                    view.lyrics_needs_scroll = true;
                    cx.notify();
                });
            })
            .map_err(|error| error.to_string())?;
        for (width, height, font) in [
            (920., 620., 16.),
            (1180., 780., 16.),
            (2048., 1222., 16.),
            (2560., 1440., 16.),
            (2048., 1222., 18.),
        ] {
            handle
                .update(cx, |_, window, cx| {
                    gpui_kit::component::Theme::update(cx, |theme| theme.font_size = px(font));
                    window.resize(size(px(width), px(height)));
                })
                .map_err(|error| error.to_string())?;
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            handle
                .update(cx, |_, window, cx| {
                    window.render_frame(cx);
                    let rem = window.rem_size();
                    let stage = window.find("expanded-stage").bounds();
                    let album = window.find("expanded-album-column").bounds();
                    let lyrics = window.find("expanded-lyrics-column").bounds();
                    let cover = window.find("expanded-cover").bounds();
                    let play = window.find("expanded-play").bounds();
                    assert!(
                        stage.size.height >= window.bounds().size.height - rem * 7.,
                        "expanded stage did not fill the window: {stage:?}"
                    );
                    assert!((album.size.height - lyrics.size.height).abs() <= px(1.));
                    assert!(lyrics.size.height >= stage.size.height - rem * 3.);
                    assert!((cover.size.width - cover.size.height).abs() <= px(1.));
                    assert!(
                        ((cover.top() + play.bottom()) * 0.5 - album.center().y).abs() <= rem * 2.,
                        "album and controls are not vertically centered"
                    );
                    assert!(stage.size.width <= rem * 96. + px(1.));
                    assert!(cover.top() >= stage.top() && play.bottom() <= stage.bottom());
                })
                .map_err(|error| error.to_string())?;
        }
    }
    screenshot(
        handle,
        &directory.join("01f-expanded-large-bilingual.png"),
        cx,
    )?;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.lyrics = original.0;
                view.lyrics_bilingual = original.1;
                view.lyrics_needs_scroll = true;
                cx.notify();
            });
            gpui_kit::component::Theme::update(cx, |theme| theme.font_size = px(16.));
            window.resize(size(px(1180.), px(780.)));
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn verify(
    handle: AnyWindowHandle,
    view: &Entity<MigApp>,
    directory: &Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    wait_for(handle, view, cx, |view| !view.loading).await?;
    let paths = local_audio_fixtures(directory)?;
    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.library.import(paths);
                view.navigate(Page::Local, window, cx);
            });
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("01-library-light.png"), cx)?;
    hover_screenshot(
        handle,
        "play-all",
        &directory.join("01a-button-hover-light.png"),
        cx,
    )
    .await?;
    hover_screenshot(
        handle,
        "favorites",
        &directory.join("01b-sidebar-hover-light.png"),
        cx,
    )
    .await?;
    hover_screenshot(
        handle,
        "play",
        &directory.join("01c-player-hover-light.png"),
        cx,
    )
    .await?;
    click(handle, "volume", cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let slider = window.find("volume-slider").bounds();
            let trigger = window.find("volume").bounds();
            assert!(slider.bottom() < trigger.origin.y);
            window.click_at(
                "volume-slider",
                point(slider.size.width * 0.3, slider.size.height * 0.5),
                cx,
            );
        })
        .map_err(|error| error.to_string())?;
    handle
        .update(cx, |_, _, cx| {
            assert!((view.read(cx).volume.read(cx).value().start() - 30.).abs() < 2.);
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("01d-volume-popover.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.press("escape", cx);
            window.render_frame(cx);
            assert!(window.try_find("volume-slider").is_none());
        })
        .map_err(|error| error.to_string())?;
    click(handle, "volume", cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("volume-slider").visible());
            assert!((view.read(cx).volume.read(cx).value().start() - 30.).abs() < 2.);
            window.click("local", cx);
            window.render_frame(cx);
            assert!(window.try_find("volume-slider").is_none());
        })
        .map_err(|error| error.to_string())?;
    click(handle, "play-all", cx)?;
    wait_for(handle, view, cx, |view| view.playing && view.position > 0.3).await?;
    handle
        .update(cx, |_, _, cx| {
            assert!(view.read(cx).duration > 31.0);
            assert!(view.read(cx).error.is_none());
        })
        .map_err(|error| error.to_string())?;
    click(handle, "open-lyrics-cover", cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(view.read(cx).expanded_lyrics);
            assert!(view.read(cx).playing);
            assert!(window.try_find("sidebar").is_none());
            assert!(window.find("close-expanded-lyrics").visible());
            window.press("escape", cx);
        })
        .map_err(|error| error.to_string())?;
    wait_for(handle, view, cx, |view| !view.expanded_lyrics).await?;
    click(handle, "open-lyrics-title", cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(view.read(cx).expanded_lyrics);
            assert!(window.find("expanded-play").visible());
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("01e-expanded-lyrics.png"), cx)?;
    verify_expanded_layout(handle, view, directory, cx).await?;
    click(handle, "close-expanded-lyrics", cx)?;
    wait_for(handle, view, cx, |view| {
        !view.expanded_lyrics && view.playing
    })
    .await?;
    click(handle, "play", cx)?;
    wait_for(handle, view, cx, |view| !view.playing).await?;
    click(handle, "current-favorite", cx)?;
    click(handle, "favorites", cx)?;
    handle
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).page, Page::Favorites);
            assert_eq!(view.read(cx).visible.len(), 1);
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("02-favorites.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.press("ctrl-f", cx);
            window.input("不存在的歌", cx);
        })
        .map_err(|error| error.to_string())?;
    wait_for(handle, view, cx, |view| {
        view.visible.is_empty() && !view.query.is_empty()
    })
    .await?;
    screenshot(handle, &directory.join("03-search-empty.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.press("ctrl-a", cx);
            window.input("晨", cx);
            window.press("space", cx);
            window.input("光", cx);
        })
        .map_err(|error| error.to_string())?;
    wait_for(handle, view, cx, |view| {
        view.query == "晨 光" && !view.playing
    })
    .await?;
    click(handle, "local", cx)?;
    wait_for(handle, view, cx, |view| {
        view.visible.len() == 3 && view.query.is_empty()
    })
    .await?;
    click(handle, "play", cx)?;
    wait_for(handle, view, cx, |view| view.playing).await?;

    let first = handle
        .update(cx, |_, _, cx| {
            view.read(cx).current.as_ref().unwrap().path.clone()
        })
        .map_err(|error| error.to_string())?;

    click(handle, "next", cx)?;
    wait_for(handle, view, cx, |view| {
        view.current
            .as_ref()
            .is_some_and(|track| track.path != first)
    })
    .await?;
    click(handle, "previous", cx)?;
    wait_for(handle, view, cx, |view| {
        view.current
            .as_ref()
            .is_some_and(|track| track.path == first)
    })
    .await?;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window.find("progress-track").bounds();
            window.click_at(
                "progress-track",
                point(bounds.size.width * 0.4, bounds.size.height * 0.5),
                cx,
            );
        })
        .map_err(|error| error.to_string())?;
    wait_for(handle, view, cx, |view| view.position >= 11.5).await?;
    click(handle, "queue", cx)?;
    handle
        .update(cx, |_, window, _| {
            assert!(window.find("close-panel").visible());
            let play = window.find("play").bounds();
            let next = window.find("next").bounds();
            assert!((play.center().y - next.center().y).abs() <= px(1.));
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("04-queue.png"), cx)?;
    click(handle, "settings", cx)?;
    click(handle, "settings-theme", cx)?;
    click(handle, "local", cx)?;
    screenshot(handle, &directory.join("05-dark.png"), cx)?;
    hover_screenshot(
        handle,
        "play-all",
        &directory.join("05a-button-hover-dark.png"),
        cx,
    )
    .await?;
    hover_screenshot(
        handle,
        "play",
        &directory.join("05b-player-hover-dark.png"),
        cx,
    )
    .await?;
    handle
        .update(cx, |_, window, _| window.resize(size(px(920.), px(620.))))
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    screenshot(handle, &directory.join("06-minimum.png"), cx)?;
    click(handle, "volume", cx)?;
    screenshot(handle, &directory.join("06a-volume-dark-minimum.png"), cx)?;
    handle
        .update(cx, |_, window, cx| window.press("escape", cx))
        .map_err(|error| error.to_string())?;
    click(handle, "close-panel", cx)?;
    handle
        .update(cx, |_, window, cx| {
            gpui_kit::component::Theme::update(cx, |theme| theme.font_size = px(18.));
            window.resize(size(px(1180.), px(780.)));
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    screenshot(handle, &directory.join("07-zoom.png"), cx)?;
    click(handle, "settings", cx)?;
    click(handle, "settings-theme", cx)?;
    click(handle, "local", cx)?;
    click(handle, "lyrics", cx)?;
    screenshot(handle, &directory.join("08-lyrics.png"), cx)?;
    click(handle, "close-panel", cx)?;
    click(handle, "settings", cx)?;
    screenshot(handle, &directory.join("09-settings.png"), cx)?;
    handle
        .update(cx, |_, window, _| window.resize(size(px(920.), px(620.))))
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    let (bottom_before, player_before) = handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);

            let bounds = (
                window.find("settings-bottom").bounds(),
                window.find("play").bounds(),
            );

            window.scroll(
                "settings-theme",
                ScrollDelta::Pixels(point(px(0.), px(-1200.))),
                cx,
            );
            bounds
        })
        .map_err(|error| error.to_string())?;

    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let bottom = window.find("settings-bottom").bounds();
            let cover = window.find("player-cover").bounds();
            assert!(bottom.origin.y < bottom_before.origin.y - px(20.));
            assert!(bottom.bottom() <= cover.origin.y);
            assert!(window.find("settings-bottom").visible());
            assert_eq!(window.find("play").bounds(), player_before);
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("09a-settings-scrolled.png"), cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.scroll(
                "settings-bottom",
                ScrollDelta::Pixels(point(px(0.), px(1200.))),
                cx,
            );
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.find("settings-bottom").bounds(), bottom_before);
            window.resize(size(px(1180.), px(780.)));
        })
        .map_err(|error| error.to_string())?;
    click(handle, "local", cx)?;
    let missing = crate::library::Track::from_path(directory.join("missing-audio.mp3"));
    let missing_id = missing.id();
    handle
        .update(cx, |_, _, cx| {
            view.update(cx, |view, cx| {
                view.library.tracks.insert(0, missing);
                view.refresh_filter();
                cx.notify();
            })
        })
        .map_err(|error| error.to_string())?;
    click(handle, SharedString::from(format!("play-{missing_id}")), cx)?;
    wait_for(handle, view, cx, |view| {
        view.error.is_some() && !view.playing
    })
    .await?;
    screenshot(handle, &directory.join("10-file-error.png"), cx)?;
    click(handle, "dismiss-error", cx)?;
    click(handle, "sources", cx)?;
    screenshot(handle, &directory.join("11-sources.png"), cx)?;
    click(handle, "online", cx)?;
    screenshot(handle, &directory.join("12-online-empty.png"), cx)?;
    click(handle, "search-kind", cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.press("down", cx);
            window.press("enter", cx);
        })
        .map_err(|error| error.to_string())?;
    wait_for(handle, view, cx, |view| {
        view.online_kind == crate::online::SearchKind::Playlists
    })
    .await?;
    screenshot(handle, &directory.join("12a-playlist-empty.png"), cx)?;
    if std::env::var_os("MIG_VERIFY_PLAYLIST").is_some() {
        handle.update(cx, |_, window, cx| {
            window.press("ctrl-f", cx);
            window.input("https://music.163.com/playlist?id=18434297757&uct2=U2FsdGVkX1+/fj0u5IwpQ+CrtcJNRTfNRV4vObptxZI=", cx);
            window.press("enter", cx);
        }).map_err(|error| error.to_string())?;
        wait_for(handle, view, cx, |view| {
            !view.searching && view.playlist_info.is_some()
        })
        .await?;
        handle
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(app.playlist_info.as_ref().unwrap().id, 18434297757);
                assert!(app.visible.len() > 10);
                assert_eq!(app.visible.len(), app.playlist_info.as_ref().unwrap().total);
            })
            .map_err(|error| error.to_string())?;
        screenshot(handle, &directory.join("15-playlist-link.png"), cx)?;
        handle
            .update(cx, |_, window, cx| {
                window.press("ctrl-f", cx);
                window.press("ctrl-a", cx);
                window.input("Outsider House", cx);
                window.press("enter", cx);
            })
            .map_err(|error| error.to_string())?;
        wait_for(handle, view, cx, |view| {
            !view.searching && !view.playlists.is_empty() && view.playlist_info.is_none()
        })
        .await?;
        screenshot(handle, &directory.join("16-playlist-search.png"), cx)?;

        let id = handle
            .update(cx, |_, _, cx| view.read(cx).playlists[0].id)
            .map_err(|error| error.to_string())?;

        click(handle, SharedString::from(format!("playlist-{id}")), cx)?;
        wait_for(handle, view, cx, |view| {
            !view.searching
                && view
                    .playlist_info
                    .as_ref()
                    .is_some_and(|info| info.id == id)
        })
        .await?;
        screenshot(handle, &directory.join("17-playlist-open.png"), cx)?;
        click(handle, "playlist-back", cx)?;
        handle
            .update(cx, |_, _, cx| {
                assert!(view.read(cx).playlist_info.is_none());
                assert!(!view.read(cx).playlists.is_empty());
            })
            .map_err(|error| error.to_string())?;
    }
    click(handle, "search-kind", cx)?;
    handle
        .update(cx, |_, window, cx| {
            window.press("up", cx);
            window.press("enter", cx);
        })
        .map_err(|error| error.to_string())?;
    wait_for(handle, view, cx, |view| {
        view.online_kind == crate::online::SearchKind::Songs
    })
    .await?;
    if std::env::var_os("MIG_VERIFY_ONLINE").is_some() {
        handle
            .update(cx, |_, window, cx| {
                window.press("ctrl-f", cx);
                window.input("晴天 周杰伦", cx);
                window.press("enter", cx);
            })
            .map_err(|error| error.to_string())?;
        wait_for(handle, view, cx, |view| {
            !view.searching && !view.visible.is_empty()
        })
        .await?;
        wait_for(handle, view, cx, |view| {
            view.visible
                .first()
                .is_some_and(|ix| view.library.tracks[*ix].cover.is_some())
        })
        .await?;
        screenshot(handle, &directory.join("13-online-results.png"), cx)?;
        click(handle, "play-all", cx)?;
        wait_for(handle, view, cx, |view| {
            view.playing
                && view.current.as_ref().is_some_and(|track| {
                    track
                        .online
                        .as_ref()
                        .is_some_and(|online| online.source == "wy")
                })
                && view.position > 0.3
                && view
                    .current
                    .as_ref()
                    .is_some_and(|track| track.cover.is_some())
        })
        .await?;
        handle
            .update(cx, |_, _, cx| {
                assert!(view.read(cx).duration > 240.);
                assert!(view.read(cx).error.is_none());
            })
            .map_err(|error| error.to_string())?;
        screenshot(handle, &directory.join("14-online-playing.png"), cx)?;
        click(handle, "play", cx)?;
        wait_for(handle, view, cx, |view| !view.playing).await?;
    }
    Ok(())
}
