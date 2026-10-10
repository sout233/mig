use super::motion::MotionButton as Button;
use super::*;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

impl MigApp {
    pub(super) fn update_search_placeholder(&self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = if self.page == Page::Library {
            "搜索在线音乐 · Enter 搜索"
        } else if self.page == Page::Online {
            match self.online_kind {
                crate::online::SearchKind::Songs => "搜索歌曲、歌手、专辑 · Enter 搜索",
                crate::online::SearchKind::Playlists => "搜索歌单或粘贴网易云歌单链接 · Enter",
            }
        } else {
            "搜索歌曲、歌手、专辑"
        };

        self.search.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx)
        });
    }

    pub(super) fn cancel_search(&mut self) {
        if let Some(flag) = self.online_cancel.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.searching = false;
    }

    pub(super) fn search_online(&mut self, page: usize, cx: &mut Context<Self>) {
        let query = if page > 1 {
            self.online_query.clone()
        } else {
            self.query.trim().to_owned()
        };

        if query.is_empty() {
            return;
        }
        if (!self.online_indices.is_empty()
            || !self.playlists.is_empty()
            || self.playlist_info.is_some())
            && (query != self.online_query || page != self.online_page)
        {
            self.remember_navigation();
        }
        self.request_online(query, page, false, cx);
    }

    pub(super) fn request_online(
        &mut self,
        query: String,
        page: usize,
        keep_query: bool,
        cx: &mut Context<Self>,
    ) {
        self.cancel_search();
        self.searching = true;
        self.error = None;
        let cancel = Arc::new(AtomicBool::new(false));
        self.online_cancel = Some(cancel.clone());
        let kind = self.online_kind;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let flag = cancel.clone();
            let requested_query = query.clone();
            let result = cx
                .background_spawn(async move {
                    crate::online::browse(&requested_query, kind, page, flag)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                this.searching = false;
                this.online_cancel = None;
                match result {
                    Ok(result) => {
                        if !keep_query {
                            this.online_query = query;
                            this.online_page = page;
                        }

                        let tracks = match result {
                            crate::online::BrowseResult::Songs(result) => {
                                this.playlist_info = None;
                                this.playlists.clear();
                                this.online_total = result.total;
                                result.tracks
                            }
                            crate::online::BrowseResult::Playlists { items, total } => {
                                this.playlist_info = None;
                                this.playlists = items;
                                this.online_total = total;
                                Vec::new()
                            }
                            crate::online::BrowseResult::Playlist(result) => {
                                if !keep_query {
                                    this.playlists.clear();
                                }
                                this.playlist_info = Some(result.info);
                                result.tracks
                            }
                        };

                        this.online_indices.clear();
                        for mut track in tracks {
                            let ix = if let Some(ix) = this
                                .library
                                .tracks
                                .iter()
                                .position(|existing| existing.path == track.path)
                            {
                                track.favorite = this.library.tracks[ix].favorite;
                                track.cover = this.library.tracks[ix].cover.clone();
                                this.library.tracks[ix] = track;
                                ix
                            } else {
                                this.library.tracks.push(track);
                                this.library.tracks.len() - 1
                            };

                            this.online_indices.push(ix);
                        }
                        this.page_transition = this.page_transition.wrapping_add(1);
                        this.sync_saved_playlist();
                        this.refresh_filter();
                        this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                    }
                    Err(error) => this.error = Some(format!("在线搜索失败：{error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn online_status(&self) -> String {
        if self.searching {
            return "正在加载…".into();
        }

        let mut status = if let Some(info) = &self.playlist_info {
            format!(
                "{} · 已加载 {} / {} 首",
                info.creator,
                self.online_indices.len(),
                info.total,
            )
        } else {
            format!(
                "共 {} {} · 第 {} 页",
                self.online_total,
                if self.online_kind == crate::online::SearchKind::Playlists {
                    "个歌单"
                } else {
                    "首"
                },
                self.online_page,
            )
        };

        if !self.playback_status.is_empty() && !self.playback_status.starts_with("当前音源：")
        {
            status.push_str(" · ");
            status.push_str(&self.playback_status);
        }
        status
    }

    pub(super) fn online_controls(&self, cx: &mut Context<Self>) -> Div {
        if self.playlist_info.is_some() && self.playlists.is_empty() && !self.searching {
            return div();
        }
        h_flex()
            .flex_shrink_0()
            .gap_2()
            .children(self.searching.then(|| {
                Button::new("cancel-search")
                    .ghost()
                    .label("取消")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel_search();
                        cx.notify();
                    }))
            }))
            .children(
                (self.playlist_info.is_some() && !self.playlists.is_empty()).then(|| {
                    Button::new("playlist-back")
                        .ghost()
                        .label("返回搜索结果")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_search();
                            this.playlist_info = None;
                            this.online_indices.clear();
                            this.refresh_filter();
                            cx.notify();
                        }))
                }),
            )
            .children(self.playlist_info.is_none().then(|| {
                Button::new("online-prev")
                    .ghost()
                    .label("上一页")
                    .disabled(self.searching || self.online_page <= 1)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.search_online(this.online_page - 1, cx)),
                    )
            }))
            .children(self.playlist_info.is_none().then(|| {
                Button::new("online-next")
                    .ghost()
                    .label("下一页")
                    .disabled(self.searching || self.online_page * 30 >= self.online_total)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.search_online(this.online_page + 1, cx)),
                    )
            }))
    }

    pub(super) fn render_playlist_results(&self, cx: &mut Context<Self>) -> Div {
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
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("歌单"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if self.online_query.is_empty() && !self.searching {
                                        "输入歌单名称搜索，或粘贴网易云歌单链接 / ID，按 Enter 加载。"
                                            .to_owned()
                                    } else {
                                        self.online_status()
                                    }),
                            ),
                    )
                    .child(self.online_controls(cx)),
            )
            .child(if self.playlists.is_empty() {
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(Icon::new(gpui_kit::assets::IconName::ListMusic).size_10())
                    .child(if self.searching {
                        "正在加载歌单…"
                    } else if self.online_query.is_empty() {
                        "找一张想听的歌单"
                    } else {
                        "没有找到匹配的歌单"
                    })
                    .into_any_element()
            } else {
                let entity = cx.entity();
                uniform_list("playlists", self.playlists.len(), move |range, _, cx| {
                    entity.update(cx, |this, cx| {
                        range
                            .map(|ix| {
                                let item = &this.playlists[ix];
                                let id = item.id;
                                Button::new(SharedString::from(format!("playlist-{id}")))
                                    .ghost()
                                    .w_full()
                                    .h(rems(4.5))
                                    .justify_start()
                                    .icon(gpui_kit::assets::IconName::ListMusic)
                                    .accessibility_label(format!("打开歌单 {}", item.name))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_2()
                                            .child(div().truncate().child(item.name.clone()))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(format!(
                                                        "{} · {} 首",
                                                        item.creator, item.total
                                                    )),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remember_navigation();
                                        this.request_online(id.to_string(), 1, true, cx);
                                    }))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .smooth_scroll("playlists-wheel")
                .flex_1()
                .min_h_0()
                .mx_6()
                .into_any_element()
            })
    }

    pub(super) fn render_sources(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p_6()
            .gap_3()
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_2()
                            .child(div().text_xl().child("内置音源"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(
                                        "默认启用 · 从上到下依次尝试 · 320kbps 优先，失败自动降级",
                                    ),
                            ),
                    )
                    .child(
                        Button::new("check-sources")
                            .label("检查音源")
                            .loading(self.probing)
                            .disabled(self.probing)
                            .on_click(cx.listener(|this, _, _, cx| this.probe_sources(cx))),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("检查仅确认脚本能否初始化；具体歌曲能否播放，以实际解析结果为准。"),
            )
            .child(
                v_flex()
                    .id("source-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .smooth_scroll("sources-wheel")
                    .gap_2()
                    .children(
                        crate::online::SOURCES
                            .iter()
                            .enumerate()
                            .map(|(ix, source)| {
                                v_flex()
                                    .py_3()
                                    .gap_2()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .child(format!("{}. {}", ix + 1, source.name))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(source.url),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                self.source_status
                                                    .get(ix)
                                                    .cloned()
                                                    .unwrap_or_else(|| "尚未检查".into()),
                                            ),
                                    )
                            }),
                    ),
            )
    }

    fn probe_sources(&mut self, cx: &mut Context<Self>) {
        if self.probing {
            return;
        }
        self.probing = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async { crate::online::probe() }).await;
            let _ = this.update(cx, |this, cx| {
                this.probing = false;
                match result {
                    Ok(status) => this.source_status = status,
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
