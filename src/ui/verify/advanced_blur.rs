use super::*;

pub(super) async fn verify(
    handle: AnyWindowHandle,
    view: &Entity<MigApp>,
    directory: &Path,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    handle.update(cx, |_, window, cx| {
        window.resize(size(px(1440.), px(960.)));
        view.update(cx, |view, cx| {
            view.current = Some(crate::library::Track::from_path("GPU blur verification.wav".into()));
            view.library.advanced_blur = None;
            view.position = 3.5;
            view.duration = 32.;
            view.lyrics = crate::library::parse_translated_lrc(
                "[00:00]A quiet opening\n[00:01]月が綺麗な夜に\n[00:02]森の中でただ一人\n[00:03]There is no one else\n[00:04]Long lyrics wrap naturally without clipping the Gaussian blur tail at the glyph boundary\n[00:05]中文翻译与日文歌词一起变模糊\n[00:06]Color emoji also blur 🎵✨\n[00:07]See you next time",
                "[00:01]月光皎洁的夜色里\n[00:02]独自一人伫立在林中\n[00:03]这里没有其他人\n[00:04]特别长的歌词可以自动换行，原文与翻译同时参与模糊效果",
            );
            view.lyrics_bilingual = true;
            view.open_expanded_lyrics(window, cx);
        });
        window.simulate_mouse_move(point(px(10.), px(10.)), cx);
    }).map_err(|error| error.to_string())?;
    settle_frames(handle, cx).await?;
    screenshot(handle, &directory.join("advanced.png"), cx)?;

    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.expanded_lyrics = false;
                view.cover_morph = None;
                view.navigate(Page::Settings, window, cx);
            });
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    click(handle, "settings-advanced-blur", cx)?;
    handle
        .update(cx, |_, window, cx| {
            assert!(!view.read(cx).library.advanced_blur_enabled());
            assert!(
                !crate::library::Library::load()
                    .unwrap()
                    .advanced_blur_enabled()
            );
            view.update(cx, |view, cx| view.open_expanded_lyrics(window, cx));
        })
        .map_err(|error| error.to_string())?;
    settle_frames(handle, cx).await?;
    screenshot(handle, &directory.join("lightweight.png"), cx)?;
    let advanced = image::open(directory.join("advanced.png"))
        .unwrap()
        .to_rgba8();
    let light = image::open(directory.join("lightweight.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(advanced.dimensions(), light.dimensions());
    let changed = advanced
        .enumerate_pixels()
        .filter(|(x, y, pixel)| {
            *x > 720 && *y > 100 && *y < 850 && **pixel != *light.get_pixel(*x, *y)
        })
        .count();
    assert!(
        changed > 1000,
        "GPU blur must visibly differ from offset-copy blur: {changed}"
    );

    handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.expanded_lyrics = false;
                view.cover_morph = None;
                view.navigate(Page::Settings, window, cx);
            });
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    click(handle, "settings-advanced-blur", cx)?;
    handle
        .update(cx, |_, _, cx| {
            assert!(view.read(cx).library.advanced_blur_enabled());
            assert!(
                crate::library::Library::load()
                    .unwrap()
                    .advanced_blur_enabled()
            );
        })
        .map_err(|error| error.to_string())?;
    screenshot(handle, &directory.join("settings.png"), cx)?;
    Ok(())
}

async fn settle_frames(handle: AnyWindowHandle, cx: &mut AsyncApp) -> Result<(), String> {
    for _ in 0..80 {
        handle
            .update(cx, |_, window, cx| window.render_frame(cx))
            .map_err(|error| error.to_string())?;
        cx.background_executor()
            .timer(Duration::from_millis(16))
            .await;
    }
    Ok(())
}
