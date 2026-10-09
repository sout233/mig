use super::motion::MotionButton as Button;
use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::scroll::ScrollableElement;

impl MigApp {
    pub(super) fn load_recommendations(&mut self, cx: &mut Context<Self>) {
        if self.recommendations_loading {
            return;
        }
        self.recommendations_requested = true;
        self.recommendations_loading = true;
        self.recommendations_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async { crate::online::recommendations() })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.recommendations_loading = false;
                match result {
                    Ok(data) => this.recommendations = Some(data),
                    Err(error) => this.recommendations_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_recommended_playlist(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page == Page::Online {
            self.remember_navigation();
        }
        self.navigate(Page::Online, window, cx);
        self.online_kind = crate::online::SearchKind::Playlists;
        self.playlist_info = None;
        self.playlists.clear();
        self.search_kind.update(cx, |state, cx| {
            state.set_selected_value(&"歌单", window, cx)
        });
        self.query = format!("https://music.163.com/playlist?id={id}");
        self.update_search_placeholder(window, cx);
        self.search.update(cx, |input, cx| {
            input.set_value(self.query.clone(), window, cx)
        });
        self.online_indices.clear();
        if let Some(saved) = self
            .library
            .playlists
            .iter()
            .find(|saved| saved.info.id == id)
        {
            self.playlist_info = Some(saved.info.clone());
            self.online_indices = saved
                .tracks
                .iter()
                .filter_map(|path| {
                    self.library
                        .tracks
                        .iter()
                        .position(|track| &track.path == path)
                })
                .collect();
        }
        self.refresh_filter();
        self.request_online(self.query.clone(), 1, false, cx);
    }

    fn play_home_tracks(&mut self, mut tracks: Vec<Track>, index: usize, cx: &mut Context<Self>) {
        for track in &mut tracks {
            if let Some(existing) = self
                .library
                .tracks
                .iter()
                .find(|item| item.path == track.path)
            {
                track.favorite = existing.favorite;
                track.cover = track.cover.clone().or_else(|| existing.cover.clone());
            } else {
                self.library.tracks.push(track.clone());
            }
        }
        if index < tracks.len() {
            self.queue = tracks;
            self.error = None;
            self.opening = true;
            self.player.send(Command::Play(self.queue.clone(), index));
            self.save();
            cx.notify();
        }
    }

    fn recommendation_art(&self, path: Option<PathBuf>, height: Rems, cx: &Context<Self>) -> Div {
        let color = cx.theme().primary;
        let fallback = move || {
            h_flex()
                .size_full()
                .justify_center()
                .child(Icon::new(Glyph::Music2).size_8().text_color(color))
                .into_any_element()
        };

        div()
            .w_full()
            .h(height)
            .flex_shrink_0()
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .bg(cx.theme().muted)
            .child(match path {
                Some(path) => img(path)
                    .size_full()
                    .rounded(cx.theme().radius)
                    .object_fit(ObjectFit::Cover)
                    .with_loading(fallback)
                    .with_fallback(fallback)
                    .into_any_element(),
                None => fallback(),
            })
    }

    fn recommended_playlist_card(
        &self,
        item: &crate::online::RecommendationPlaylist,
        featured: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let id = item.id;

        let contents = v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(self.recommendation_art(
                item.cover.clone(),
                rems(if featured { 12. } else { 8. }),
                cx,
            ))
            .child(
                div()
                    .w_full()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(item.name.clone()),
            )
            .child(
                div()
                    .w_full()
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(if featured {
                        item.description.clone()
                    } else {
                        "精选歌单".into()
                    }),
            );

        let card = Button::new(SharedString::from(format!(
            "recommended-playlist-{id}-{featured}"
        )))
        .ghost()
        .h_auto()
        .p_2()
        .justify_start()
        .accessibility_label(format!("打开歌单 {}", item.name))
        .tooltip(item.name.clone())
        .child(contents)
        .on_click(
            cx.listener(move |this, _, window, cx| this.open_recommended_playlist(id, window, cx)),
        );

        if featured {
            card.flex_1().min_w_0()
        } else {
            card.w(rems(9.)).flex_shrink_0()
        }
    }

    fn home_track_row(
        &self,
        id: &'static str,
        tracks: &[Track],
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        h_flex()
            .id(id)
            .w_full()
            .min_w_0()
            .gap_3()
            .overflow_x_scroll()
            .items_start()
            .children(tracks.iter().enumerate().map(|(index, track)| {
                let queue = tracks.to_vec();
                Button::new(SharedString::from(format!("{id}-{}", track.id())))
                    .ghost()
                    .w(rems(9.))
                    .h_auto()
                    .p_2()
                    .flex_shrink_0()
                    .justify_start()
                    .accessibility_label(format!("播放 {}", track.title))
                    .tooltip(track.title.clone())
                    .child(
                        v_flex()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .child(self.recommendation_art(track.cover.clone(), rems(8.), cx))
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(track.title.clone()),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(track.artist.clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.play_home_tracks(queue.clone(), index, cx)
                    }))
            }))
    }

    fn recommendation_heading(title: &str, cx: &Context<Self>) -> Div {
        div()
            .text_lg()
            .font_weight(FontWeight::BOLD)
            .text_color(cx.theme().foreground)
            .child(title.to_owned())
    }

    pub(super) fn render_recommendations(&self, cx: &mut Context<Self>) -> Div {
        let mut body = v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .p_6()
            .gap_6()
            .child(
                h_flex()
                    .w_full()
                    .gap_4()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(div().text_2xl().font_weight(FontWeight::BOLD).child("推荐"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(self.tip),
                            ),
                    )
                    .child(
                        Button::new("refresh-recommendations")
                            .ghost()
                            .icon(Glyph::RefreshCw)
                            .label(if self.recommendations_loading {
                                "正在更新"
                            } else {
                                "刷新推荐"
                            })
                            .disabled(self.recommendations_loading)
                            .on_click(cx.listener(|this, _, _, cx| this.load_recommendations(cx))),
                    ),
            );

        if let Some(message) = self.recommendations_error.as_ref().or_else(|| {
            self.recommendations
                .as_ref()
                .and_then(|data| data.notice.as_ref())
        }) {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(message.clone()),
            );
        }
        if self.recommendations.is_none() && self.recommendations_error.is_none() {
            body = body.child(
                v_flex()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("正在加载推荐…"),
                    )
                    .child(h_flex().gap_4().children((0..2).map(|_| {
                        div()
                            .flex_1()
                            .h(rems(12.))
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().muted.opacity(0.4))
                    }))),
            );
        }
        if let Some(data) = &self.recommendations {
            if !data.playlists.is_empty() {
                body = body.child(
                    v_flex()
                        .gap_3()
                        .child(Self::recommendation_heading("精选推荐", cx))
                        .child(
                            h_flex().w_full().min_w_0().gap_4().items_start().children(
                                data.playlists
                                    .iter()
                                    .take(2)
                                    .map(|item| self.recommended_playlist_card(item, true, cx)),
                            ),
                        ),
                );
            }
            if data.playlists.len() > 2 {
                body =
                    body.child(
                        v_flex()
                            .gap_3()
                            .child(Self::recommendation_heading("热门歌单", cx))
                            .child(
                                h_flex()
                                    .id("recommended-playlists")
                                    .w_full()
                                    .min_w_0()
                                    .gap_3()
                                    .items_start()
                                    .overflow_x_scroll()
                                    .children(data.playlists.iter().skip(2).map(|item| {
                                        self.recommended_playlist_card(item, false, cx)
                                    })),
                            ),
                    );
            }
            if !data.tracks.is_empty() {
                body = body.child(
                    v_flex()
                        .gap_3()
                        .child(Self::recommendation_heading("新歌速递", cx))
                        .child(self.home_track_row("recommended-songs", &data.tracks, cx)),
                );
            }
        }

        let recent: Vec<_> = self
            .library
            .recent
            .iter()
            .take(8)
            .filter_map(|path| {
                self.library
                    .tracks
                    .iter()
                    .find(|track| &track.path == path)
                    .cloned()
            })
            .collect();

        if !recent.is_empty() {
            body = body.child(
                v_flex()
                    .gap_3()
                    .child(Self::recommendation_heading("继续聆听", cx))
                    .child(self.home_track_row("home-recent", &recent, cx)),
            );
        }
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(body.overflow_y_scrollbar().id("recommendations-scroll"))
    }
}
