use super::motion::MotionButton as Button;
use super::*;
use crate::library::time_label;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{input::Input, popover::Popover, select::Select, slider::Slider};
use gpui_kit::prelude::FluentBuilder;

impl MigApp {
    fn render_workspace(&self, compact: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let rem = window.rem_size();

        let sidebar_width = self
            .library
            .sidebar_width
            .filter(|width| width.is_finite())
            .unwrap_or(13.)
            .clamp(10., 24.);

        let panel_width = self
            .library
            .panel_width
            .filter(|width| width.is_finite())
            .unwrap_or(13.)
            .clamp(11., 28.);

        let content = gpui_kit::base::animation::EffectTransition::new(Duration::from_millis(210))
            .fade(0.0, 1.0)
            .slide_y(px(5.), px(0.))
            .apply(
                self.render_content(compact, cx).relative(),
                SharedString::from(format!("page-transition-{}", self.page_transition)),
            );

        let content = if let Some(queue) = self.panel {
            h_resizable("content-panel-split")
                .with_state(&self.panel_split)
                .child(
                    resizable_panel()
                        .size_range(rem * 24. ..Pixels::MAX)
                        .child(content),
                )
                .child(
                    resizable_panel()
                        .size(rem * panel_width)
                        .size_range(rem * 11. ..rem * 28.)
                        .flex_none()
                        .child(
                            gpui_kit::base::animation::EffectTransition::new(
                                Duration::from_millis(190),
                            )
                            .fade(0.0, 1.0)
                            .slide_x(px(6.), px(0.))
                            .apply(
                                self.render_panel(queue, cx).relative(),
                                if queue {
                                    "queue-reveal"
                                } else {
                                    "lyrics-reveal"
                                },
                            ),
                        ),
                )
                .on_resize(
                    cx.listener(|this, state: &Entity<ResizableState>, window, cx| {
                        if let Some(width) = state.read(cx).sizes().get(1) {
                            this.library.panel_width = Some(*width / window.rem_size());
                            this.save();
                        }
                    }),
                )
                .into_any_element()
        } else {
            content.into_any_element()
        };

        div().flex().flex_1().min_h_0().min_w_0().child(
            h_resizable("sidebar-content-split")
                .with_state(&self.sidebar_split)
                .child(
                    resizable_panel()
                        .size(rem * sidebar_width)
                        .size_range(rem * 10. ..rem * 24.)
                        .flex_none()
                        .child(self.sidebar(cx)),
                )
                .child(
                    resizable_panel()
                        .size_range(rem * if self.panel.is_some() { 35. } else { 24. }..Pixels::MAX)
                        .child(
                            v_flex()
                                .size_full()
                                .min_w_0()
                                .min_h_0()
                                .child(self.toolbar(window, cx))
                                .child(div().flex().flex_1().min_h_0().min_w_0().child(content)),
                        ),
                )
                .on_resize(
                    cx.listener(|this, state: &Entity<ResizableState>, window, cx| {
                        if let Some(width) = state.read(cx).sizes().first() {
                            this.library.sidebar_width = Some(*width / window.rem_size());
                            this.save();
                        }
                    }),
                ),
        )
    }

    fn navigation(
        &self,
        id: &'static str,
        page: Page,
        icon: Glyph,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .ghost()
            .accessibility_label(page.title())
            .child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .child(Icon::new(icon).small())
                    .child(page.title()),
            )
            .selected(self.page == page)
            .w_full()
            .justify_start()
            .on_click(cx.listener(move |this, _, window, cx| this.navigate(page, window, cx)))
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("sidebar")
            .w_full()
            .flex_shrink_0()
            .h_full()
            .p_4()
            .gap_4()
            .overflow_y_scroll()
            .smooth_scroll("sidebar-wheel")
            .bg(cx.theme().transparent)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("音乐库"),
                    )
                    .child(self.navigation("library", Page::Library, Glyph::Sparkles, cx))
                    .child(self.navigation("online", Page::Online, Glyph::Search, cx))
                    .child(self.navigation("recent", Page::Recent, Glyph::Clock3, cx))
                    .child(self.navigation("local", Page::Local, Glyph::Folder, cx)),
            )
            .child(self.playlist_favorites(cx))
            .child(v_flex().flex_shrink_0().gap_3().child(self.navigation(
                "settings",
                Page::Settings,
                Glyph::Settings2,
                cx,
            )))
    }

    fn toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let focused = self
            .search
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);

        let search_width = gpui_kit::base::motion::transition(
            "search-width",
            window.rem_size() * if focused { 23. } else { 14. },
            gpui_kit::base::motion::Transition::new(Duration::from_millis(220)),
            window,
            cx,
        );

        h_flex()
            .h_16()
            .flex_shrink_0()
            .px_6()
            .gap_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Button::new("navigate-back")
                            .ghost()
                            .icon(Glyph::ChevronLeft)
                            .accessibility_label("后退")
                            .tooltip("后退 · Alt+←")
                            .disabled(self.navigation_back.is_empty())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.browse_history(false, window, cx)
                            })),
                    )
                    .child(
                        Button::new("navigate-forward")
                            .ghost()
                            .icon(Glyph::ChevronRight)
                            .accessibility_label("前进")
                            .tooltip("前进 · Alt+→")
                            .disabled(self.navigation_forward.is_empty())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.browse_history(true, window, cx)
                            })),
                    ),
            )
            .children((self.page != Page::Settings).then(|| {
                div()
                    .w(search_width)
                    .min_w_0()
                    .max_w(rems(23.))
                    .child(super::motion::field(
                        Input::new(&self.search)
                            .prefix(Icon::new(Glyph::Search).small())
                            .cleanable(true),
                        "search-field",
                    ))
            }))
            .children((self.page == Page::Online).then(|| {
                div()
                    .w(rems(6.))
                    .h_9()
                    .flex_shrink_0()
                    .child(super::motion::field(
                        Select::new(&self.search_kind)
                            .id("search-kind")
                            .w_full()
                            .accessibility_label("搜索类型"),
                        "search-kind-field",
                    ))
            }))
            .child(div().flex_1())
    }

    fn player_bar(&self, background: Hsla, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let track = self.current.as_ref();
        let path = track.map(|track| track.path.clone());
        let playing = self.playing;
        let volume = self.volume.clone();
        let cover_bounds = self.bottom_cover_bounds.clone();
        let cover_opacity = if cx.reduce_motion() {
            if self.expanded_lyrics { 0. } else { 1. }
        } else {
            self.bottom_cover_opacity()
        };
        if !cx.reduce_motion() && cover_opacity > 0. && cover_opacity < 1. {
            window.request_animation_frame();
        }
        h_flex()
            .flex_shrink_0()
            .h(rems(5.5))
            .pl_5()
            .pr_6()
            .gap_10()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(background)
            .child(
                h_flex()
                    .gap_3()
                    .w(rems(15.))
                    .min_w_0()
                    .child(
                        div().flex_shrink_0().opacity(cover_opacity).child(
                            Button::new("open-lyrics-cover")
                                .ghost()
                                .cover_overlay()
                                .on_prepaint(move |bounds, _, _| cover_bounds.set(Some(bounds)))
                                .p_0()
                                .size(rems(3.875))
                                .disabled(track.is_none())
                                .accessibility_label("展开歌词")
                                .child(self.album_cover(track, true, cx))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_expanded_lyrics(window, cx)
                                })),
                        ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                Button::new("open-lyrics-title")
                                    .ghost()
                                    .w_full()
                                    .min_w_0()
                                    .h(rems(1.5))
                                    .p_0()
                                    .justify_start()
                                    .disabled(track.is_none())
                                    .accessibility_label("展开当前歌曲歌词")
                                    .child(self.player_title(track))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_expanded_lyrics(window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        track
                                            .map(|track| track.artist.clone())
                                            .unwrap_or("滚木滚木滚木".into()),
                                    ),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_4()
                            .child(
                                Button::new("mode")
                                    .ghost()
                                    .small()
                                    .icon(if self.mode == PlayMode::Shuffle {
                                        Glyph::Shuffle
                                    } else {
                                        Glyph::Repeat
                                    })
                                    .selected(self.mode != PlayMode::Sequential)
                                    .tooltip(self.mode.label())
                                    .accessibility_label(self.mode.label())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.mode = this.mode.next();
                                        this.player.send(Command::Mode(this.mode));
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("previous")
                                    .ghost()
                                    .icon(Icon::default().path("icons/mig/previous-solid.svg"))
                                    .disabled(self.queue.is_empty())
                                    .tooltip("上一首 · Ctrl+←")
                                    .accessibility_label("上一首")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.player.send(Command::Previous)
                                    })),
                            )
                            .child(
                                Button::new("play")
                                    .ghost()
                                    .large()
                                    .when(!self.opening, |button| {
                                        button.icon(if playing {
                                            Icon::default().path("icons/mig/pause-solid.svg")
                                        } else {
                                            Icon::default().path("icons/mig/play-solid.svg")
                                        })
                                    })
                                    .children(self.opening.then(|| {
                                        gpui_kit::component::spinner::Spinner::new()
                                            .with_size(cx.theme().font_size * 1.5)
                                            .color(cx.theme().primary)
                                    }))
                                    .disabled(self.loading || self.library.tracks.is_empty())
                                    .tooltip(if self.opening {
                                        "取消在线加载 · Space"
                                    } else {
                                        "播放 / 暂停 · Space"
                                    })
                                    .accessibility_label(if self.opening {
                                        "取消加载"
                                    } else if playing {
                                        "暂停"
                                    } else {
                                        "播放"
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_playback(cx)),
                                    ),
                            )
                            .child(
                                Button::new("next")
                                    .ghost()
                                    .icon(Icon::default().path("icons/mig/next-solid.svg"))
                                    .disabled(self.queue.is_empty())
                                    .tooltip("下一首 · Ctrl+→")
                                    .accessibility_label("下一首")
                                    .on_click(
                                        cx.listener(|this, _, _, _| {
                                            this.player.send(Command::Next)
                                        }),
                                    ),
                            )
                            .child(
                                Button::new("lyrics")
                                    .ghost()
                                    .small()
                                    .icon(Glyph::TextQuote)
                                    .selected(self.panel == Some(false))
                                    .tooltip("歌词")
                                    .accessibility_label("显示歌词")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.panel = if this.panel == Some(false) {
                                            None
                                        } else {
                                            Some(false)
                                        };
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(div().w_10().child(time_label(self.position)))
                            .child(
                                div()
                                    .id("progress-track")
                                    .flex_1()
                                    .child(
                                        super::progress::ProgressSlider::new(
                                            &self.progress,
                                            "bottom-progress",
                                        )
                                        .disabled(self.duration <= 0.),
                                    )
                                    .test_support(),
                            )
                            .child(div().w_10().child(time_label(self.duration))),
                    ),
            )
            .child(
                h_flex()
                    .w(rems(7.5))
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("current-favorite")
                            .ghost()
                            .icon(Self::favorite_icon(
                                track.is_some_and(|track| track.favorite),
                            ))
                            .disabled(track.is_none())
                            .accessibility_label("收藏当前歌曲")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(path) = &path {
                                    this.favorite(path.clone(), cx);
                                }
                            })),
                    )
                    .child(
                        Popover::new("volume-popover")
                            .anchor(Anchor::BottomRight)
                            .trigger(
                                Button::new("volume")
                                    .ghost()
                                    .icon(if self.volume.read(cx).value().start() == 0. {
                                        Glyph::VolumeX
                                    } else {
                                        Glyph::Volume2
                                    })
                                    .tooltip("音量")
                                    .accessibility_label("调整音量"),
                            )
                            .content(move |_, _, cx| {
                                v_flex()
                                    .w(rems(12.))
                                    .child(
                                        div()
                                            .id("volume-slider")
                                            .test_support()
                                            .w_full()
                                            .py_2()
                                            .child(Slider::new(&volume)),
                                    )
                                    .text_color(cx.theme().popover_foreground)
                            }),
                    )
                    .child(
                        Button::new("queue")
                            .ghost()
                            .icon(Glyph::ListMusic)
                            .selected(self.panel == Some(true))
                            .tooltip("播放队列")
                            .accessibility_label("显示播放队列")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.panel = if this.panel == Some(true) {
                                    None
                                } else {
                                    Some(true)
                                };
                                cx.notify();
                            })),
                    ),
            )
    }

    pub(super) fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.library.dark = !self.library.dark;
        crate::theme::apply_cover(self.library.dark, self.theme_accent, cx);
        self.save();
        cx.notify();
    }
}

impl Render for MigApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_expanded_lyrics(window, cx);
        let background = crate::theme::background(window, cx);
        let search = self.search.clone();
        let search_visible = self.page != Page::Settings && !self.expanded_lyrics;
        let compact = self.panel.is_some() || window.bounds().size.width < window.rem_size() * 65.;
        v_flex()
            .id("music-app")
            .relative()
            .key_context("Music")
            .track_focus(&self.focus)
            .size_full()
            .bg(background.gradient)
            .text_color(cx.theme().foreground)
            .text_sm()
            .on_action(
                cx.listener(|this, _: &ImportMusic, window, cx| {
                    this.import_local(false, window, cx)
                }),
            )
            .on_action(move |_: &SearchMusic, window, cx| {
                if search_visible {
                    search.update(cx, |input, cx| input.focus(window, cx));
                }
            })
            .on_action(cx.listener(|this, _: &CloseLyrics, window, cx| {
                this.close_expanded_lyrics(window, cx)
            }))
            .on_action(cx.listener(|this, _: &NavigateBack, window, cx| {
                this.browse_history(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NavigateForward, window, cx| {
                this.browse_history(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| this.toggle_playback(cx)))
            .on_action(cx.listener(|this, _: &NextTrack, _, _| this.player.send(Command::Next)))
            .on_action(
                cx.listener(|this, _: &PreviousTrack, _, _| this.player.send(Command::Previous)),
            )
            .child(
                TitleBar::new()
                    .border_b_0()
                    .bg(cx.theme().transparent)
                    .children((!self.expanded_lyrics).then(|| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                self.current
                                    .as_ref()
                                    .map(|track| format!("MIG - {}", track.title))
                                    .unwrap_or_else(|| "MIG ~".into()),
                            )
                    })),
            )
            .child({
                let mut stage = v_flex().relative().flex_1().min_h_0().min_w_0();
                if !self.expanded_lyrics || self.cover_morph.is_some() {
                    stage = stage.child(
                        v_flex()
                            .size_full()
                            .min_h_0()
                            .opacity(1. - self.expanded_part_visibility(0))
                            .child(self.render_workspace(compact, window, cx))
                            .child(self.player_bar(background.surface, window, cx)),
                    );
                }
                if self.expanded_lyrics {
                    stage = stage.child(
                        v_flex()
                            .absolute()
                            .inset_0()
                            .occlude()
                            .child(self.render_expanded_lyrics(window, cx)),
                    );
                }
                stage
            })
            .children(self.error.clone().map(|error| {
                h_flex()
                    .px_6()
                    .py_2()
                    .gap_3()
                    .bg(cx.theme().danger.opacity(0.08))
                    .text_color(cx.theme().danger)
                    .child(
                        v_flex()
                            .id("error-detail")
                            .flex_1()
                            .min_w_0()
                            .max_h(rems(6.))
                            .overflow_y_scroll()
                            .smooth_scroll("error-detail-wheel")
                            .child(error),
                    )
                    .child(
                        Button::new("dismiss-error")
                            .ghost()
                            .small()
                            .icon(Glyph::X)
                            .accessibility_label("关闭提示")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            })),
                    )
            }))
            .children(self.render_cover_morph(window, cx))
            .children(self.queue_popup.as_ref().map(|popup| {
                deferred(
                    anchored().child(
                        div()
                            .w(window.bounds().size.width)
                            .h(window.bounds().size.height)
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(
                                anchored()
                                    .position(popup.position)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(popup.menu.clone()),
                            ),
                    ),
                )
                .with_priority(gpui_kit::base::POPUP_PRIORITY)
            }))
    }
}
