use super::motion::MotionButton as Button;
use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};

pub(super) struct QueuePopup {
    pub(super) menu: Entity<PopupMenu>,
    pub(super) position: Point<Pixels>,
    pub(super) anchor: Anchor,
    previous_focus: Option<FocusHandle>,
    _subscription: Subscription,
}

#[derive(Clone, Copy)]
enum QueueAction {
    Play,
    Next,
    Favorite,
    Remove,
    KeepOnly,
    Clear,
}

impl MigApp {
    fn queue_action(
        &mut self,
        path: &std::path::Path,
        action: QueueAction,
        cx: &mut Context<Self>,
    ) {
        let ix = self.queue.iter().position(|track| track.path == path);
        if matches!(action, QueueAction::Clear) {
            self.queue.clear();
        } else {
            let Some(ix) = ix else {
                return;
            };

            match action {
                QueueAction::Play => {
                    self.player.send(Command::Play(self.queue.clone(), ix));
                    self.opening = true;
                    self.error = None;
                    cx.notify();
                    return;
                }
                QueueAction::Favorite => {
                    self.favorite(path.to_path_buf(), cx);
                    return;
                }
                QueueAction::Next => {
                    if self.opening {
                        return;
                    }
                    let current = self.current.as_ref().map(|track| track.path.clone());
                    if current.as_deref() == Some(path) {
                        return;
                    }
                    let track = self.queue.remove(ix);

                    let next = current
                        .and_then(|path| self.queue.iter().position(|t| t.path == path))
                        .map(|ix| ix + 1)
                        .unwrap_or(0);

                    self.queue.insert(next, track);
                    self.player
                        .send(Command::PlayNext(self.queue.clone(), path.to_path_buf()));
                    self.save();
                    cx.notify();
                    return;
                }
                QueueAction::Remove => {
                    self.queue.remove(ix);
                }
                QueueAction::KeepOnly => {
                    let track = self.queue[ix].clone();
                    self.queue = vec![track];
                }
                QueueAction::Clear => unreachable!(),
            }
        }
        self.player.send(Command::Queue(self.queue.clone()));
        self.save();
        cx.notify();
    }

    fn open_queue_menu(
        &mut self,
        path: PathBuf,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_focus = match self.queue_popup.take() {
            Some(popup) => popup.previous_focus,
            None => window.focused(cx),
        };
        let entity = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            let menu = match previous_focus.clone() {
                Some(focus) => menu.action_context(focus),
                None => menu,
            };
            self.queue_menu(path, entity, menu)
        });
        self.present_track_popup(menu, position, Anchor::TopLeft, previous_focus, window, cx);
    }

    pub(super) fn present_track_popup(
        &mut self,
        menu: Entity<PopupMenu>,
        position: Point<Pixels>,
        anchor: Anchor,
        previous_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        menu.focus_handle(cx).focus(window, cx);
        let subscription =
            cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
                let popup = this
                    .queue_popup
                    .take()
                    .expect("dismiss the active track menu");
                if popup.menu.focus_handle(cx).is_focused(window) {
                    match popup.previous_focus {
                        Some(focus) => window.focus(&focus, cx),
                        None => window.blur(cx),
                    }
                }
                cx.notify();
            });
        self.queue_popup = Some(QueuePopup {
            menu,
            position,
            anchor,
            previous_focus,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn queue_menu(
        &self,
        path: PathBuf,
        entity: WeakEntity<Self>,
        mut menu: PopupMenu,
    ) -> PopupMenu {
        let favorite = self
            .queue
            .iter()
            .find(|track| track.path == path)
            .is_some_and(|track| track.favorite);
        let selected = self
            .current
            .as_ref()
            .is_some_and(|track| track.path == path);
        let opening = self.opening;

        for (label, action, disabled) in [
            ("立即播放", QueueAction::Play, false),
            ("下一首播放", QueueAction::Next, selected || opening),
            (
                if favorite { "取消收藏" } else { "收藏" },
                QueueAction::Favorite,
                false,
            ),
            ("移出队列", QueueAction::Remove, false),
            ("仅保留这首", QueueAction::KeepOnly, false),
            ("清空队列", QueueAction::Clear, false),
        ] {
            if matches!(action, QueueAction::Remove | QueueAction::Clear) {
                menu = menu.separator();
            }
            let entity = entity.clone();
            let path = path.clone();
            menu = menu.item(PopupMenuItem::new(label).disabled(disabled).on_click(
                move |_, _, cx| {
                    entity
                        .update(cx, |this, cx| this.queue_action(&path, action, cx))
                        .expect("queue view dropped while its menu is open");
                },
            ));
        }

        menu
    }

    pub(super) fn render_panel(&self, queue: bool, cx: &mut Context<Self>) -> Div {
        let mut panel = v_flex()
            .w_full()
            .flex_shrink_0()
            .min_h_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().transparent)
            .child(
                h_flex()
                    .p_4()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if queue {
                                format!("播放队列 · {}", self.queue.len())
                            } else {
                                "歌词".into()
                            }),
                    )
                    .children(queue.then(|| {
                        Button::new("clear-queue")
                            .ghost()
                            .small()
                            .icon(Glyph::Trash)
                            .disabled(self.queue.is_empty())
                            .tooltip("清空队列")
                            .accessibility_label("清空播放队列")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.queue_action(std::path::Path::new(""), QueueAction::Clear, cx);
                            }))
                    }))
                    .child(
                        Button::new("close-panel")
                            .ghost()
                            .small()
                            .icon(Glyph::X)
                            .accessibility_label("关闭侧栏")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.panel = None;
                                cx.notify();
                            })),
                    ),
            );

        if queue {
            let entity = cx.entity();
            panel = panel.child(
                v_flex()
                    .id("queue-context-menu")
                    .test_support()
                    .capture_any_mouse_down(|event, window, _| {
                        if event.button == MouseButton::Right {
                            window.prevent_default();
                        }
                    })
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list("queue-list", self.queue.len(), move |range, _, cx| {
                            entity.update(cx, |this, cx| {
                                range
                                    .map(|ix| {
                                        let track = &this.queue[ix];
                                        let selected = this
                                            .current
                                            .as_ref()
                                            .is_some_and(|current| current.path == track.path);

                                        let path = track.path.clone();
                                        div()
                                            .id(SharedString::from(format!(
                                                "queue-row-{}",
                                                track.id()
                                            )))
                                            .h_16()
                                            .px_3()
                                            .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                this.open_queue_menu(path.clone(), event.position, window, cx);
                                                cx.stop_propagation();
                                            }))
                                            .child(
                                                Button::new(SharedString::from(format!(
                                                    "queued-{}",
                                                    track.id()
                                                )))
                                                .ghost()
                                                .selected(selected)
                                                .w_full()
                                                .h(rems(3.5))
                                                .justify_start()
                                                .accessibility_label(format!(
                                                    "播放 {}",
                                                    track.title
                                                ))
                                                .child(
                                                    v_flex()
                                                        .w_full()
                                                        .min_w_0()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .truncate()
                                                                .child(track.title.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(
                                                                    cx.theme().muted_foreground,
                                                                )
                                                                .truncate()
                                                                .child(track.artist.clone()),
                                                        ),
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.player.send(Command::Play(
                                                        this.queue.clone(),
                                                        ix,
                                                    ));
                                                    this.opening = true;
                                                    cx.notify();
                                                })),
                                            )
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .smooth_scroll("queue-wheel")
                        .flex_1()
                        .min_h_0(),
                    ),
            );
            if self.queue.is_empty() {
                panel = panel.child(
                    div()
                        .p_4()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("啥也没有。。。"),
                );
            }
        } else {
            panel = panel.child(
                v_flex()
                    .p_4()
                    .gap_2()
                    .child(
                        div().text_lg().font_weight(FontWeight::MEDIUM).child(
                            self.current
                                .as_ref()
                                .map(|track| track.title.clone())
                                .unwrap_or("waiting4love".into()),
                        ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                self.current
                                    .as_ref()
                                    .map(|track| track.artist.clone())
                                    .unwrap_or_default(),
                            ),
                    ),
            );
            if self.lyrics.is_empty() {
                panel = panel.child(
                    v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .p_6()
                        .gap_4()
                        .child(
                            Icon::new(Glyph::AudioLines)
                                .size_8()
                                .text_color(cx.theme().primary),
                        )
                        .child(div().child("暂无歌词"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("tip：可以把同名 .lrc 放在音频旁"),
                        ),
                );
            } else {
                let timed = self.lyrics[0].seconds.is_finite();

                let active = self
                    .lyrics
                    .partition_point(|line| line.seconds <= self.position)
                    .saturating_sub(1);

                panel = panel.child(
                    v_flex()
                        .id("panel-lyrics-text")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .smooth_scroll("panel-lyrics-wheel")
                        .p_6()
                        .gap_6()
                        .children(
                            self.lyrics
                                .iter()
                                .enumerate()
                                .skip(if timed { active.saturating_sub(2) } else { 0 })
                                .take(if timed { 5 } else { self.lyrics.len() })
                                .map(|(ix, line)| {
                                    v_flex()
                                        .gap_1()
                                        .text_sm()
                                        .font_weight(if timed && ix == active {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .text_color(if timed && ix == active {
                                            cx.theme().primary
                                        } else {
                                            cx.theme().muted_foreground
                                        })
                                        .child(line.text.clone())
                                        .children(line.translation.clone().map(|translation| {
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::NORMAL)
                                                .text_color(cx.theme().muted_foreground)
                                                .child(translation)
                                        }))
                                }),
                        ),
                );
            }
        }
        panel
    }
}
