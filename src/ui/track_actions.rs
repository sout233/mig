use super::motion::MotionButton as Button;
use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{
    menu::{PopupMenu, PopupMenuItem},
    notification::Notification,
};

impl MigApp {
    pub(super) fn track_more_button(&self, cx: &mut Context<Self>) -> Div {
        let bounds = std::rc::Rc::new(std::cell::Cell::new(None::<Bounds<Pixels>>));
        let measured_bounds = bounds.clone();
        let button = Button::new("current-more").ghost();
        let button = if self.downloading {
            button.child(gpui_kit::component::spinner::Spinner::new().with_size(px(18.)))
        } else {
            button.icon(Glyph::Ellipsis)
        };
        let button = button
            .disabled(self.current.is_none())
            .tooltip(if self.downloading {
                "正在下载…"
            } else {
                "更多歌曲操作"
            })
            .accessibility_label("更多歌曲操作")
            .on_click(cx.listener(move |this, _, window, cx| {
                let bounds = bounds.get().expect("more button has been laid out");
                let position = point(bounds.right(), bounds.top() - window.rem_size() * 0.5);
                this.open_current_menu(position, window, cx);
            }));
        div()
            .flex_shrink_0()
            .on_prepaint(move |bounds, _, _| measured_bounds.set(Some(bounds)))
            .child(button)
    }

    fn open_current_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(track) = self.current.clone() else {
            return;
        };
        let previous_focus = window.focused(cx);
        let owner = cx.entity().downgrade();
        let downloading = self.downloading;
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            let mut menu = menu;
            for (label, action, disabled) in [
                ("添加到收藏夹…", TrackAction::Collection, false),
                (
                    if downloading {
                        "正在下载…"
                    } else {
                        "下载…"
                    },
                    TrackAction::Download,
                    track.online.is_none() || downloading,
                ),
                ("属性", TrackAction::Properties, false),
            ] {
                let owner = owner.clone();
                let track = track.clone();
                menu = menu.item(PopupMenuItem::new(label).disabled(disabled).on_click(
                    move |_, window, cx| {
                        let owner = owner.clone();
                        let track = track.clone();
                        // Let the menu dismiss before opening the next dialog
                        window.defer(cx, move |window, cx| {
                            owner
                                .update(cx, |this, cx| match action {
                                    TrackAction::Collection => {
                                        this.choose_collection(track, window, cx)
                                    }
                                    TrackAction::Download => this.download_track(track, window, cx),
                                    TrackAction::Properties => {
                                        this.track_properties(track, window, cx)
                                    }
                                })
                                .expect("track menu owner dropped");
                        });
                    },
                ));
            }
            menu
        });
        self.present_track_popup(
            menu,
            position,
            Anchor::BottomRight,
            previous_focus,
            window,
            cx,
        );
    }

    fn track_properties(&self, track: Track, window: &mut Window, cx: &mut Context<Self>) {
        let location = track.path.display().to_string();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("歌曲属性").child(
                v_flex()
                    .gap_3()
                    .child(format!("标题：{}", track.title))
                    .child(format!("歌手：{}", track.artist))
                    .child(format!("专辑：{}", track.album))
                    .child(format!(
                        "时长：{}",
                        track
                            .duration
                            .map(crate::library::time_label)
                            .unwrap_or("未知".into())
                    ))
                    .child(if let Some(online) = &track.online {
                        format!("在线平台：{}", online.source)
                    } else {
                        format!("文件：{location}")
                    }),
            )
        });
    }

    fn download_track(&mut self, track: Track, window: &mut Window, cx: &mut Context<Self>) {
        if self.downloading {
            return;
        }
        self.downloading = true;
        cx.notify();
        window.push_notification(Notification::new().message("正在获取下载音频…"), cx);
        cx.spawn_in(window, async move |this, cx| {
            let source_track = track.clone();
            let prepared = cx
                .background_spawn(async move { crate::download::prepare(&source_track) })
                .await;
            let result = match prepared {
                Ok(download) => {
                    let name = format!(
                        "{}.{}",
                        crate::download::filename(&track),
                        download.extension
                    );
                    let destination = rfd::AsyncFileDialog::new()
                        .set_title("保存歌曲")
                        .set_file_name(&name)
                        .add_filter("音频", &[download.extension])
                        .save_file()
                        .await;
                    match destination {
                        Some(destination) => {
                            let path = destination.path().to_owned();
                            cx.background_spawn(async move {
                                crate::download::save(download, &track, &path)?;
                                Ok(Some(path))
                            })
                            .await
                        }
                        None => Ok(None),
                    }
                }
                Err(error) => Err(error),
            };
            this.update_in(cx, |this, window, cx| {
                this.downloading = false;
                match result {
                    Ok(Some(path)) => window.push_notification(
                        Notification::success(format!("已下载：{}", path.display())),
                        cx,
                    ),
                    Ok(None) => {}
                    Err(error) => {
                        this.error = Some(format!("下载失败：{error}"));
                    }
                }
                cx.notify();
            })
            .expect("download owner dropped");
        })
        .detach();
    }
}

#[derive(Clone, Copy)]
enum TrackAction {
    Collection,
    Download,
    Properties,
}
