use super::motion::MotionButton as Button;
use super::*;
use crate::library::time_label;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{popover::Popover, slider::Slider};
use gpui_kit::prelude::FluentBuilder;

pub(super) struct ExpandedTransition {
    source: Option<Bounds<Pixels>>,
    started: std::time::Instant,
    pub(super) closing: bool,
    from: [f32; 7],
}

impl ExpandedTransition {
    fn duration(&self) -> f32 {
        if self.closing { 0.85 } else { 0.70 }
    }
}

// Start fast, then take our time settling into place
// maybe i called gpt to finish quart easing but it didn't
fn expand_ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    (1. - (-8. * t).exp()) / (1. - (-8.0f32).exp())
}

impl MigApp {
    pub(super) fn open_expanded_lyrics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current.is_none()
            || (self.expanded_lyrics && !self.cover_morph.as_ref().is_some_and(|m| m.closing))
        {
            return;
        }

        let source = self
            .cover_transition_bounds()
            .or(self.bottom_cover_bounds.get());

        let from = std::array::from_fn(|ix| self.expanded_part_visibility(ix));
        self.cover_morph = (!cx.reduce_motion()).then(|| ExpandedTransition {
            source,
            started: std::time::Instant::now(),
            closing: false,
            from,
        });
        let opacity = self.bottom_cover_opacity();
        self.bottom_cover_fade = Some((opacity, 0., std::time::Instant::now()));
        self.expanded_lyrics = true;
        self.lyrics_follow = true;
        self.lyrics_needs_scroll = true;
        self.lyrics_layout = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn close_expanded_lyrics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.expanded_lyrics || self.cover_morph.as_ref().is_some_and(|m| m.closing) {
            return;
        }

        let source = self
            .cover_transition_bounds()
            .or(self.expanded_cover_bounds.get());

        let from = std::array::from_fn(|ix| self.expanded_part_visibility(ix));
        let opacity = self.bottom_cover_opacity();
        self.bottom_cover_fade = Some((opacity, 1., std::time::Instant::now()));
        if cx.reduce_motion() {
            self.expanded_lyrics = false;
            self.cover_morph = None;
        } else {
            self.cover_morph = Some(ExpandedTransition {
                source,
                started: std::time::Instant::now(),
                closing: true,
                from,
            });
        }
        self.lyrics_scroll_motion = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn expanded_part_visibility(&self, part: usize) -> f32 {
        let Some(motion) = &self.cover_morph else {
            return if self.expanded_lyrics { 1. } else { 0. };
        };

        let delay = if motion.closing {
            0.
        } else {
            [0., 0.02, 0.06, 0.10, 0.14, 0.18, 0.08][part]
        };

        let t = expand_ease(
            (motion.started.elapsed().as_secs_f32() - delay) / (motion.duration() - delay),
        );

        let target = if motion.closing { 0. } else { 1. };
        motion.from[part] + (target - motion.from[part]) * t
    }

    fn expanded_cover(&self, size: Pixels, cx: &Context<Self>) -> impl IntoElement {
        let foreground = cx.theme().primary;
        let cover_bounds = self.expanded_cover_bounds.clone();

        let fallback = move || {
            h_flex()
                .size_full()
                .justify_center()
                .child(
                    Icon::new(Glyph::Disc3)
                        .size(rems(5.))
                        .text_color(foreground),
                )
                .into_any_element()
        };

        div()
            .id("expanded-cover")
            .on_prepaint(move |bounds, _, _| cover_bounds.set(Some(bounds)))
            .opacity(
                if self
                    .cover_morph
                    .as_ref()
                    .is_some_and(|motion| motion.source.is_some())
                {
                    0.
                } else {
                    self.expanded_part_visibility(0)
                },
            )
            .test_support()
            .size(size)
            .flex_shrink_0()
            .rounded(cx.theme().radius_lg)
            .overflow_hidden()
            .shadow_lg()
            .bg(cx.theme().muted)
            .child(
                if let Some(path) = self.current.as_ref().and_then(|track| track.cover.clone()) {
                    img(path)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .rounded(cx.theme().radius_lg)
                        .with_loading(fallback)
                        .with_fallback(fallback)
                        .into_any_element()
                } else {
                    fallback()
                },
            )
    }

    fn expanded_controls(&self, cx: &mut Context<Self>) -> Div {
        let path = self.current.as_ref().map(|track| track.path.clone());
        let volume = self.volume.clone();
        h_flex()
            .w_full()
            .justify_center()
            .gap_2()
            .child(
                Button::new("expanded-mode")
                    .ghost()
                    .small()
                    .mr_4()
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
                Button::new("expanded-previous")
                    .ghost()
                    .large()
                    .icon(Icon::default().path("icons/mig/previous-solid.svg"))
                    .disabled(self.queue.is_empty())
                    .accessibility_label("上一首")
                    .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Previous))),
            )
            .child(
                Button::new("expanded-play")
                    .ghost()
                    .large()
                    .size_12()
                    .p_0()
                    .when(!self.opening, |button| {
                        button.icon(
                            Icon::default()
                                .path(if self.playing {
                                    "icons/mig/pause-solid.svg"
                                } else {
                                    "icons/mig/play-solid.svg"
                                })
                                .size_8(),
                        )
                    })
                    .children(self.opening.then(|| {
                        gpui_kit::component::spinner::Spinner::new()
                            .with_size(cx.theme().font_size * 2.)
                            .color(cx.theme().primary)
                    }))
                    .tooltip(if self.opening {
                        "正在加载 · 点击取消"
                    } else {
                        "播放 / 暂停"
                    })
                    .accessibility_label(if self.opening {
                        "取消加载"
                    } else if self.playing {
                        "暂停"
                    } else {
                        "播放"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_playback(cx))),
            )
            .child(
                Button::new("expanded-next")
                    .ghost()
                    .large()
                    .mr_4()
                    .icon(Icon::default().path("icons/mig/next-solid.svg"))
                    .disabled(self.queue.is_empty())
                    .accessibility_label("下一首")
                    .on_click(cx.listener(|this, _, _, _| this.player.send(Command::Next))),
            )
            .child(
                Button::new("expanded-favorite")
                    .ghost()
                    .small()
                    .icon(Self::favorite_icon(
                        self.current.as_ref().is_some_and(|track| track.favorite),
                    ))
                    .accessibility_label(
                        if self.current.as_ref().is_some_and(|track| track.favorite) {
                            "取消收藏当前歌曲"
                        } else {
                            "收藏当前歌曲"
                        },
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(path) = &path {
                            this.favorite(path.clone(), cx);
                        }
                    })),
            )
            .child(
                Popover::new("expanded-volume-popover")
                    .anchor(Anchor::BottomRight)
                    .trigger(
                        Button::new("expanded-volume")
                            .ghost()
                            .small()
                            .icon(if self.volume.read(cx).value().start() <= 0. {
                                Glyph::VolumeX
                            } else {
                                Glyph::Volume2
                            })
                            .accessibility_label("调整音量"),
                    )
                    .content(move |_, _, _| {
                        v_flex()
                            .w(rems(12.))
                            .p_4()
                            .gap_3()
                            .child(Slider::new(&volume))
                    }),
            )
    }

    pub(super) fn render_expanded_lyrics(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = window.bounds().size.width < window.rem_size() * 70.;

        let cover_size = (window.bounds().size.height - window.rem_size() * 21.)
            .min(window.bounds().size.width * 0.36)
            .clamp(window.rem_size() * 10., window.rem_size() * 26.);

        let track = self.current.as_ref();
        let padding = self.lyrics_padding(window);
        let title_reveal = self.expanded_part_visibility(2);
        let metadata_reveal = self.expanded_part_visibility(3);
        let progress_reveal = self.expanded_part_visibility(4);
        let controls_reveal = self.expanded_part_visibility(5);
        let lyrics_reveal = self.expanded_part_visibility(6);
        let mut right = v_flex()
            .id("expanded-lyrics-column")
            .relative()
            .left(px(36.) * (1. - lyrics_reveal))
            .opacity(lyrics_reveal)
            .test_support()
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0();

        if self.lyrics.is_empty() {
            right = right.justify_center().gap_4().child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("暂无歌词"),
            );
        } else {
            let handle = self.lyrics_scroll.0.borrow().base_handle.clone();
            let edge = window.rem_size() * self.lyrics_row_height() * padding as f32;
            let viewport = if handle.bounds().size.height > px(0.) {
                handle.bounds().size.height
            } else {
                window.bounds().size.height
            };
            let range = self
                .lyrics_geometry
                .visible(-handle.offset().y - edge, viewport);
            let top = edge + self.lyrics_geometry.top(range.start);
            let bottom = edge + self.lyrics_geometry.total() - self.lyrics_geometry.top(range.end);
            let expected_width = self.lyrics_geometry.width();
            let inset = window.rem_size() * 1.5 + px(2.);
            let rows = range
                .map(|ix| self.expanded_lyric_row(ix, window, cx))
                .collect::<Vec<_>>();

            right = right.child(
                v_flex()
                    .id("expanded-lyric-lines")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(div().h(top).flex_shrink_0())
                    .children(rows)
                    .child(div().h(bottom).flex_shrink_0())
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                if (bounds.size.width - inset - expected_width).abs() > px(0.5) {
                                    window.request_animation_frame();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                        this.lyrics_follow = false;
                        this.lyrics_scroll_motion = None;
                        cx.notify();
                    }))
                    .smooth_scroll_with("lyrics-wheel", &self.lyrics_scroll),
            );
            right = right.child(
                h_flex().h_10().justify_end().children(
                    (!self.lyrics_follow
                        && self
                            .lyrics
                            .first()
                            .is_some_and(|line| line.seconds.is_finite()))
                    .then(|| {
                        Button::new("lyrics-follow")
                            .ghost()
                            .small()
                            .label("回到当前歌词")
                            .icon(Glyph::LocateFixed)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.lyrics_follow = true;
                                this.lyrics_needs_scroll = true;
                                cx.notify();
                            }))
                    }),
                ),
            );
        }

        let body = v_flex()
            .items_center()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(
                h_flex()
                    .w_full()
                    .h_10()
                    .flex_shrink_0()
                    .px_2()
                    .opacity(self.expanded_part_visibility(1))
                    .child(
                        Button::new("close-expanded-lyrics")
                            .ghost()
                            .icon(Glyph::ChevronDown)
                            .accessibility_label("收起歌词")
                            .tooltip("收起歌词 · Esc")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_expanded_lyrics(window, cx)
                            })),
                    ),
            )
            .child(
                h_flex()
                    .id("expanded-stage")
                    .test_support()
                    .w_full()
                    .max_w(rems(96.))
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .px(if compact { rems(2.) } else { rems(4.) })
                    .pb_8()
                    .gap(if compact { rems(3.) } else { rems(6.) })
                    .child(
                        v_flex()
                            .id("expanded-album-column")
                            .test_support()
                            .w(cover_size)
                            .h_full()
                            .flex_shrink_0()
                            .justify_center()
                            .gap_5()
                            .child(self.expanded_cover(cover_size, cx))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .relative()
                                            .top(px(22.) * (1. - title_reveal))
                                            .opacity(title_reveal)
                                            .text_xl()
                                            .line_clamp(2)
                                            .font_weight(FontWeight::BOLD)
                                            .child(
                                                track
                                                    .map(|track| track.title.clone())
                                                    .unwrap_or_default(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .top(px(26.) * (1. - metadata_reveal))
                                            .opacity(metadata_reveal)
                                            .text_sm()
                                            .truncate()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                track
                                                    .map(|track| {
                                                        format!(
                                                            "{} · {}",
                                                            track.artist, track.album
                                                        )
                                                    })
                                                    .unwrap_or_default(),
                                            ),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .relative()
                                    .top(px(30.) * (1. - progress_reveal))
                                    .opacity(progress_reveal)
                                    .gap_1()
                                    .child(
                                        super::progress::ProgressSlider::new(
                                            &self.progress,
                                            "expanded-progress",
                                        )
                                        .disabled(self.duration <= 0.),
                                    )
                                    .child(
                                        h_flex()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(time_label(self.position))
                                            .child(div().flex_1())
                                            .child(format!(
                                                "−{}",
                                                time_label((self.duration - self.position).max(0.))
                                            )),
                                    ),
                            )
                            .child(
                                self.expanded_controls(cx)
                                    .relative()
                                    .top(px(34.) * (1. - controls_reveal))
                                    .opacity(controls_reveal),
                            ),
                    )
                    .child(right),
            );

        v_flex().flex_1().min_h_0().min_w_0().child(body)
    }
}

impl MigApp {
    fn lyrics_row_height(&self) -> f32 {
        let base = if self.lyrics_bilingual { 7.2 } else { 4.8 };
        base + (self.library.lyrics_spacing_percent() - 100.) * 0.015
    }

    fn lyrics_padding(&self, window: &Window) -> usize {
        if self
            .lyrics
            .first()
            .is_some_and(|line| !line.seconds.is_finite())
        {
            return 0;
        }
        ((window.bounds().size.height / (window.rem_size() * self.lyrics_row_height() * 2.)).ceil()
            as usize)
            .max(1)
    }

    fn expanded_lyric_row(
        &self,
        line_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line = &self.lyrics[line_ix];
        let seconds = line.seconds;
        let active = self.lyrics_active == Some(line_ix);
        let distance = line_ix.abs_diff(self.lyrics_active.unwrap_or(0)).min(5);
        let browsing = !self.lyrics_follow || !line.seconds.is_finite();
        let emphasis = gpui_kit::base::motion::transition(
            SharedString::from(format!("lyric-emphasis-{line_ix}")),
            if active {
                1.0f32
            } else if browsing {
                0.8
            } else {
                (0.62 - distance as f32 * 0.07).max(0.28)
            },
            gpui_kit::base::motion::Transition::new(Duration::from_millis(180)),
            window,
            cx,
        );

        let blur = gpui_kit::base::motion::transition(
            SharedString::from(format!("lyric-blur-{line_ix}")),
            if active || browsing {
                0.0f32
            } else {
                0.08 + distance as f32 * 0.035
            },
            gpui_kit::base::motion::Transition::new(Duration::from_millis(280)),
            window,
            cx,
        );

        let scale = gpui_kit::base::motion::transition(
            SharedString::from(format!("lyric-scale-{line_ix}")),
            if active || browsing { 1.0f32 } else { 0.96 },
            gpui_kit::base::motion::Transition::new(Duration::from_millis(420))
                .ease(super::lyrics_visual::scroll_ease),
            window,
            cx,
        );

        let shift = gpui_kit::base::motion::transition(
            SharedString::from(format!("lyric-shift-{line_ix}")),
            if browsing {
                0.0f32
            } else {
                ((line_ix as f32 - self.lyrics_active.unwrap_or(0) as f32) * 0.07)
                    .clamp(-0.28, 0.28)
            },
            gpui_kit::base::motion::Transition::new(Duration::from_millis(420))
                .ease(super::lyrics_visual::scroll_ease)
                .delay(gpui_kit::base::motion::SignedDuration::positive(
                    Duration::from_millis(if browsing { 0 } else { distance as u64 * 35 }),
                )),
            window,
            cx,
        );

        let original: SharedString = if line.text.trim().is_empty() {
            "♪".to_owned()
        } else {
            line.text.clone()
        }
        .into();

        let translation = line.translation.clone().map(SharedString::from);
        Button::new(SharedString::from(format!("expanded-line-{line_ix}")))
            .ghost()
            .w_full()
            .h(self.lyrics_geometry.height(line_ix))
            .flex_shrink_0()
            .px_3()
            .justify_start()
            .disabled(!seconds.is_finite())
            .accessibility_label(if seconds.is_finite() {
                format!("跳到 {}：{}", time_label(seconds), line.text)
            } else {
                line.text.clone()
            })
            .tooltip(match &line.translation {
                Some(translation) => format!("{}\n{}", line.text, translation),
                None => line.text.clone(),
            })
            .child(
                div()
                    .size_full()
                    .relative()
                    .top(rems(shift))
                    .opacity(emphasis)
                    .child(super::lyrics_visual::lyric_text(
                        SharedString::from(format!("lyric-layout-{line_ix}")),
                        original,
                        translation,
                        blur,
                        scale,
                    )),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.position = seconds;
                this.progress.update(cx, |state, cx| {
                    state.set_value(
                        if this.duration > 0. {
                            (seconds / this.duration * 100.) as f32
                        } else {
                            0.
                        },
                        window,
                        cx,
                    );
                });
                this.player.send(Command::Seek(seconds));
                this.lyrics_follow = true;
                this.lyrics_needs_scroll = true;
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn sync_expanded_lyrics(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.expanded_lyrics || self.lyrics.is_empty() {
            return;
        }

        let handle = self.lyrics_scroll.0.borrow().base_handle.clone();
        let compact = window.bounds().size.width < window.rem_size() * 70.;
        let cover = (window.bounds().size.height - window.rem_size() * 21.)
            .min(window.bounds().size.width * 0.36)
            .clamp(window.rem_size() * 10., window.rem_size() * 26.);
        let fallback = window.bounds().size.width.min(window.rem_size() * 96.)
            - cover
            - window.rem_size() * if compact { 7. } else { 14. };
        let width = if handle.bounds().size.width > px(0.) {
            handle.bounds().size.width
        } else {
            fallback
        };
        let width = (width - window.rem_size() * 1.5 - px(2.)).max(px(1.));
        if self.lyrics_geometry.update(
            &self.lyrics,
            width,
            self.library.lyrics_spacing_percent(),
            window,
        ) {
            self.lyrics_needs_scroll = true;
        }
        if !self.lyrics[0].seconds.is_finite() {
            return;
        }

        let active = self
            .lyrics
            .partition_point(|line| line.seconds <= self.position)
            .checked_sub(1);

        let changed = active != self.lyrics_active;
        self.lyrics_active = active;
        let viewport = handle.bounds().size.height;
        if viewport <= px(0.) {
            window.request_animation_frame();
            return;
        }

        let layout = (viewport, window.rem_size());
        let resized = self.lyrics_layout != Some(layout);
        self.lyrics_layout = Some(layout);
        if !self.lyrics_follow {
            if changed {
                window.request_animation_frame();
            }
            return;
        }
        if viewport > px(0.) && (changed || resized || self.lyrics_needs_scroll) {
            self.lyrics_needs_scroll = false;
            let padding = self.lyrics_padding(window);
            let row_height = window.rem_size() * self.lyrics_row_height();
            let edge = row_height * padding as f32;
            let total_height = self.lyrics_geometry.total() + edge * 2.;
            let minimum = -(total_height - viewport).max(px(0.));
            let index = active.unwrap_or(0);
            let target = (viewport * 0.5
                - edge
                - self.lyrics_geometry.top(index)
                - self.lyrics_geometry.height(index) * 0.5)
                .clamp(minimum, px(0.));

            if cx.reduce_motion() {
                handle.set_offset(point(px(0.), target));
                self.lyrics_scroll_motion = None;
            } else {
                self.lyrics_scroll_motion =
                    Some((handle.offset().y, target, std::time::Instant::now()));
            }
            window.request_animation_frame();
        }
        if let Some((from, to, started)) = self.lyrics_scroll_motion {
            let progress = (started.elapsed().as_secs_f32() / 0.6).min(1.);
            let eased = super::lyrics_visual::scroll_ease(progress);
            handle.set_offset(point(px(0.), from + (to - from) * eased));
            if progress >= 1. {
                self.lyrics_scroll_motion = None;
            }
            window.request_animation_frame();
        } else if changed {
            window.request_animation_frame();
        }
    }
}

impl MigApp {
    pub(super) fn bottom_cover_opacity(&self) -> f32 {
        let Some((from, to, started)) = self.bottom_cover_fade else {
            return 1.;
        };
        let duration = if to < from { 0.15 } else { 1.0 };
        let progress = (started.elapsed().as_secs_f32() / duration).min(1.);
        from + (to - from) * progress
    }

    pub(super) fn cover_morph_active(&self) -> bool {
        self.cover_morph
            .as_ref()
            .is_some_and(|motion| motion.started.elapsed().as_secs_f32() < motion.duration())
    }

    fn cover_transition_bounds(&self) -> Option<Bounds<Pixels>> {
        let motion = self.cover_morph.as_ref()?;
        let source = motion.source?;

        let target = if motion.closing {
            self.bottom_cover_bounds.get().unwrap_or(source)
        } else {
            self.expanded_cover_bounds.get().unwrap_or(source)
        };

        let elapsed = motion.started.elapsed().as_secs_f32();

        let (origin, side) = if motion.closing {
            let t = expand_ease(elapsed / motion.duration());
            (
                source.origin + (target.origin - source.origin) * t,
                source.size.width + (target.size.width - source.size.width) * t,
            )
        } else {
            let small = source.size.width * 0.94;
            let small_origin = source.origin
                + point(
                    (source.size.width - small) * 0.5,
                    (source.size.height - small) * 0.5,
                );

            if elapsed < 0.08 {
                let t = expand_ease(elapsed / 0.08);
                (
                    source.origin + (small_origin - source.origin) * t,
                    source.size.width + (small - source.size.width) * t,
                )
            } else {
                let t = expand_ease((elapsed - 0.08) / (motion.duration() - 0.08));
                (
                    small_origin + (target.origin - small_origin) * t,
                    small + (target.size.width - small) * t,
                )
            }
        };

        Some(Bounds::new(origin, size(side, side)))
    }

    pub(super) fn render_cover_morph(
        &self,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> Option<Div> {
        // Hold the last frame until the timer finishes the transition
        let bounds = self.cover_transition_bounds()?;
        let origin = bounds.origin;
        let side = bounds.size.width;
        window.request_animation_frame();

        let artwork =
            if let Some(path) = self.current.as_ref().and_then(|track| track.cover.clone()) {
                img(path)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .into_any_element()
            } else {
                h_flex()
                    .size_full()
                    .justify_center()
                    .child(
                        Icon::new(Glyph::Disc3)
                            .size(side * 0.2)
                            .text_color(cx.theme().primary),
                    )
                    .into_any_element()
            };

        Some(
            div()
                .absolute()
                .left(origin.x)
                .top(origin.y)
                .size(side)
                .rounded(cx.theme().radius_lg)
                .overflow_hidden()
                .bg(cx.theme().muted)
                .child(artwork),
        )
    }
}
