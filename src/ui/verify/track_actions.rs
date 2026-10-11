use super::*;
use gpui_kit::component::WindowExt;

pub(super) async fn verify(
    handle: AnyWindowHandle,
    view: &Entity<MigApp>,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let id = handle
        .update(cx, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.panel = None;
                view.current = Some(view.queue[0].clone());
                let name = format!(
                    "UI collection {}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                );
                let id = view.library.create_collection(&name).unwrap();
                view.navigate(Page::Local, window, cx);
                id
            })
        })
        .map_err(|error| error.to_string())?;
    click(handle, "current-more", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    keys(handle, &["down", "enter"], cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));
            assert!(window.try_find("popup-menu").is_none());
            window.click(SharedString::from(format!("choose-collection-{id}")), cx);
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(!window.has_active_dialog(cx));
            assert_eq!(
                view.read(cx)
                    .library
                    .collections
                    .iter()
                    .find(|item| item.id == id)
                    .unwrap()
                    .tracks
                    .len(),
                1
            );
            window.click(SharedString::from(format!("local-collection-{id}")), cx);
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(view.read(cx).page, Page::Collection(id));
            assert_eq!(view.read(cx).visible.len(), 1);
            let track = &view.read(cx).library.tracks[view.read(cx).visible[0]];
            window.click(
                SharedString::from(format!("remove-collection-{}", track.id())),
                cx,
            );
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, _, cx| assert!(view.read(cx).visible.is_empty()))
        .map_err(|error| error.to_string())?;
    click(handle, "current-more", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    // The download item is disabled for this local fixture
    keys(handle, &["down", "down", "enter"], cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));
            assert!(window.try_find("popup-menu").is_none());
            window.close_dialog(cx);
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    click(handle, "rename-collection", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.click("collection-name", cx);
            window.press("ctrl-a", cx);
            window.input("Renamed collection", cx);
            window.click("save-local-collection", cx);
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(!window.has_active_dialog(cx));
            assert_eq!(
                view.read(cx)
                    .library
                    .collections
                    .iter()
                    .find(|item| item.id == id)
                    .unwrap()
                    .name,
                "Renamed collection"
            );
        })
        .map_err(|error| error.to_string())?;
    click(handle, "delete-collection", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    click(handle, "confirm-delete-collection", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(!window.has_active_dialog(cx));
            assert!(
                !view
                    .read(cx)
                    .library
                    .collections
                    .iter()
                    .any(|item| item.id == id)
            );
            assert_eq!(view.read(cx).page, Page::Favorites);
        })
        .map_err(|error| error.to_string())?;
    click(handle, "new-local-collection", cx)?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.click("collection-name", cx);
            window.input(&format!("Created collection {id}"), cx);
            window.click("save-local-collection", cx);
        })
        .map_err(|error| error.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(!window.has_active_dialog(cx));
            assert!(
                view.read(cx)
                    .library
                    .collections
                    .iter()
                    .any(|item| item.name == format!("Created collection {id}"))
            );
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn keys(handle: AnyWindowHandle, keys: &[&str], cx: &mut AsyncApp) -> Result<(), String> {
    handle
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("popup-menu").is_some());
            for key in keys {
                let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
                window.dispatch_event(
                    gpui_kit::PlatformInput::KeyDown(gpui_kit::KeyDownEvent {
                        keystroke: keystroke.clone(),
                        is_held: false,
                        prefer_character_input: false,
                    }),
                    cx,
                );
                window.dispatch_event(
                    gpui_kit::PlatformInput::KeyUp(gpui_kit::KeyUpEvent { keystroke }),
                    cx,
                );
            }
        })
        .map_err(|error| error.to_string())
}
