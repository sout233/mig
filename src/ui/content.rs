use super::motion::MotionButton as Button;
use super::*;
use crate::library::time_label;
use gpui_kit::assets::IconName as Glyph;

impl MigApp {
    pub(super) fn render_content(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        if self.page == Page::Library {
            return self.render_recommendations(cx);
        }
        if self.page == Page::Settings {
            return self.render_settings(cx);
        }
        if self.page == Page::Sources {
            return self.render_sources(cx);
        }
        if self.page == Page::Online
            && self.online_kind == crate::online::SearchKind::Playlists
            && self.playlist_info.is_none()
        {
            return self.render_playlist_results(cx);
        }

        let title = if let Some(info) = self
            .playlist_info
            .as_ref()
            .filter(|_| self.page == Page::Online)
        {
            info.name.clone()
        } else if let Page::Collection(id) = self.page {
            self.library
                .collections
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.name.clone())
                .unwrap_or_default()
        } else if self.query.is_empty() {
            self.page.title().to_owned()
        } else {
            format!("搜索“{}”", self.query)
        };

        let description = if self.page == Page::Online {
            self.online_status()
        } else {
            match self.page {
                Page::Local => self.local_import_status.as_deref().unwrap_or(self.tip),
                _ => self.tip,
            }
            .to_owned()
        };

        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                h_flex()
                    .px_6()
                    .pt_6()
                    .pb_4()
                    .gap_4()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(
                                h_flex().gap_3().child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description),
                            ),
                    )
                    .children((self.page == Page::Online).then(|| self.online_controls(cx)))
                    .children(self.playlist_favorite_button(cx))
                    .children(self.collection_controls(cx))
                    .child(
                        Button::new("play-all")
                            .label("播放全部")
                            .icon(Glyph::Play)
                            .disabled(self.visible.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.play_visible(0, cx))),
                    ),
            )
            .children(
                (self.page == Page::Local).then(|| self.local_import_buttons(cx).px_6().pb_4()),
            )
            .child(
                h_flex()
                    .mx_6()
                    .px_3()
                    .h_9()
                    .gap_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(div().w_6().child("#"))
                    .child(div().flex_1().min_w_0().child("标题"))
                    .child(
                        div()
                            .w(relative(if compact { 0.25 } else { 0.3 }))
                            .flex_shrink_0()
                            .min_w_0()
                            .child("专辑"),
                    )
                    .child(div().w_12().text_right().child("时长"))
                    .child(div().w_8())
                    .children(matches!(self.page, Page::Collection(_)).then(|| div().w_8())),
            )
            .child(if self.visible.is_empty() {
                self.empty_state(cx).into_any_element()
            } else {
                let entity = cx.entity();
                uniform_list("tracks", self.visible.len(), move |range, _, cx| {
                    entity.update(cx, |this, cx| {
                        range
                            .map(|ix| this.track_row(ix, compact, cx))
                            .collect::<Vec<_>>()
                    })
                })
                .smooth_scroll_with("tracks-wheel", &self.scroll)
                .flex_1()
                .min_h_0()
                .mx_6()
                .into_any_element()
            })
    }

    fn track_row(&self, ix: usize, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let track = &self.library.tracks[self.visible[ix]];
        let path = track.path.clone();
        let selected = self
            .current
            .as_ref()
            .is_some_and(|current| current.path == track.path);

        let title = track.title.clone();

        let artist = if track.artist.trim().is_empty() {
            "未知歌手".to_owned()
        } else {
            track.artist.clone()
        };

        let album = if track.album.trim().is_empty() {
            "—".to_owned()
        } else {
            track.album.clone()
        };

        let row_id: ElementId = SharedString::from(format!("row-{}", track.id())).into();

        let idle = if selected {
            cx.theme().primary.opacity(0.08)
        } else {
            cx.theme().transparent
        };

        let row = h_flex()
            .id(row_id.clone())
            .w_full()
            .h_16()
            .gap_3()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.55))
            .bg(idle)
            .child(
                div()
                    .w_6()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if selected && self.playing {
                        "♫".into()
                    } else {
                        format!("{:02}", ix + 1)
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("play-{}", track.id())))
                    .ghost()
                    .quiet_surface()
                    .flex_1()
                    .min_w_0()
                    .h_12()
                    .justify_start()
                    .px_0()
                    .accessibility_label(format!("播放 {}", track.title))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .min_w_0()
                            .child(self.album_cover(Some(track), false, cx))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(
                                        div()
                                            .truncate()
                                            .font_weight(if selected {
                                                FontWeight::SEMIBOLD
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(artist),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.play_visible(ix, cx);
                    })),
            )
            .child(
                div()
                    .w(relative(if compact { 0.25 } else { 0.3 }))
                    .flex_shrink_0()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(album),
            )
            .child(
                div()
                    .w_12()
                    .text_right()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(track.duration.map(time_label).unwrap_or("—".into())),
            )
            .child(
                Button::new(SharedString::from(format!("favorite-{}", track.id())))
                    .ghost()
                    .small()
                    .w_8()
                    .icon(Self::favorite_icon(track.favorite))
                    .accessibility_label(if track.favorite {
                        "取消收藏"
                    } else {
                        "收藏歌曲"
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.favorite(path.clone(), cx))),
            );

        let row = if matches!(self.page, Page::Collection(_)) {
            let path = track.path.clone();
            row.child(
                Button::new(SharedString::from(format!(
                    "remove-collection-{}",
                    track.id()
                )))
                .ghost()
                .small()
                .icon(Glyph::X)
                .tooltip("移出收藏夹")
                .accessibility_label("移出收藏夹")
                .on_click(
                    cx.listener(move |this, _, _, cx| this.remove_collection_track(&path, cx)),
                ),
            )
        } else {
            row
        };
        super::motion::row(row, row_id, idle).into_any_element()
    }

    fn empty_state(&self, cx: &mut Context<Self>) -> Div {
        let (title, description) = if self.page == Page::Online && self.searching {
            ("正在搜索在线音乐", "可以取消搜索，或等待结果返回。")
        } else if self.page == Page::Online && self.online_query.is_empty() {
            ("搜索你想听的音乐", "在上方输入歌曲或歌手，按 Enter 搜索。")
        } else if self.loading {
            ("正在读取音乐库", "稍等片刻，音乐马上就来。")
        } else if !self.query.is_empty() {
            ("没有找到匹配的歌曲", "试试其他关键词，或清除搜索条件。")
        } else if self.page == Page::Favorites {
            (
                "还没有收藏的歌曲",
                "点击歌曲旁的爱心，把喜欢的音乐放在这里~",
            )
        } else if self.page == Page::Recent {
            ("从第一首音乐开始", "哇哇哇哇哇")
        } else {
            ("还没有本地音乐", "使用上方的导入按钮添加各种各样的文件")
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap_4()
            .child(
                Icon::new(Glyph::Music2)
                    .size_10()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(div().text_lg().child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(description),
            )
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> Div {
        let content = v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .p_6()
            .gap_6()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("设置"),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("其实没啥用"),
            )
            .child(
                h_flex()
                    .py_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex().flex_1().gap_2().child("外观").child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("黑白配"),
                        ),
                    )
                    .child(
                        Button::new("settings-theme")
                            .label(if self.library.dark {
                                "切换浅色"
                            } else {
                                "切换深色"
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_theme(cx))),
                    ),
            )
            .child(
                h_flex()
                    .py_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex().flex_1().gap_2().child("播放模式").child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("唉……想当年我听BadApple DJ版循环了一夜一夜又一夜……"),
                        ),
                    )
                    .child(
                        Button::new("settings-mode")
                            .label(self.mode.label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = this.mode.next();
                                this.player.send(Command::Mode(this.mode));
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .py_4()
                    .gap_6()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex().flex_1().min_w_0().gap_2().child("歌词行距").child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("调整全屏歌词的行间距离"),
                        ),
                    )
                    .child(
                        v_flex()
                            .w(rems(14.))
                            .flex_shrink_0()
                            .gap_2()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{:.0}%",
                                                self.library.lyrics_spacing_percent()
                                            )),
                                    )
                                    .child(
                                        Button::new("reset-lyrics-spacing")
                                            .ghost()
                                            .small()
                                            .label("重置")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.library.lyrics_spacing = None;
                                                this.lyrics_spacing.update(cx, |state, cx| {
                                                    state.set_value(100., window, cx)
                                                });
                                                this.lyrics_needs_scroll = true;
                                                this.lyrics_layout = None;
                                                this.lyrics_scroll_motion = None;
                                                this.save();
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(gpui_kit::component::slider::Slider::new(
                                &self.lyrics_spacing,
                            )),
                    ),
            )
            .child(
                h_flex()
                    .py_4()
                    .gap_6()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child("使用高级模糊")
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "一百万以内最好的GPU Filter。有这么好的模糊效果怎么不早点告诉我？",
                                    ),
                            ),
                    )
                    .child(
                        gpui_kit::component::switch::Switch::new("settings-advanced-blur")
                            .accessibility_label("使用高级模糊")
                            .checked(self.library.advanced_blur_enabled())
                            .disabled(!cfg!(target_os = "windows"))
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.library.advanced_blur = Some(*checked);
                                this.save();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                v_flex().gap_3().child("键盘快捷键").children(
                    [
                        "Space  播放 / 暂停",
                        "Ctrl+← / Ctrl+→  上一首 / 下一首",
                        "Ctrl+O  导入音乐",
                        "Ctrl+F  搜索音乐",
                    ]
                    .into_iter()
                    .map(|text| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(text)
                    }),
                ),
            )
            .child(
                v_flex()
                    .gap_3()
                    .pt_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child("关于 MIG 音乐")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Powered by GPUI Kit"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("dev by sout"),
                    ),
            );

        div().flex_1().min_w_0().min_h_0().child(
            content
                .id("settings-scroll")
                .overflow_y_scroll()
                .smooth_scrollbar("settings-wheel"),
        )
    }
}
