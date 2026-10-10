use super::motion::MotionButton as Button;
use super::*;
use gpui_kit::assets::IconName as Glyph;

impl MigApp {
    pub(super) fn playlist_favorite_button(&self, cx: &mut Context<Self>) -> Option<Button> {
        let info = self
            .playlist_info
            .as_ref()
            .filter(|_| self.page == Page::Online)?;

        let saved = self
            .library
            .playlists
            .iter()
            .any(|item| item.info.id == info.id);

        Some(
            Button::new("favorite-playlist")
                .icon(Self::favorite_icon(saved))
                .label(if saved { "已收藏" } else { "收藏歌单" })
                .accessibility_label(if saved {
                    "取消收藏歌单"
                } else {
                    "收藏歌单"
                })
                .tooltip(if saved {
                    "取消收藏歌单"
                } else {
                    "收藏到左侧栏"
                })
                .disabled(self.searching)
                .on_click(cx.listener(|this, _, _, cx| this.toggle_playlist_favorite(cx))),
        )
    }

    fn toggle_playlist_favorite(&mut self, cx: &mut Context<Self>) {
        let Some(info) = self.playlist_info.clone() else {
            return;
        };

        if let Some(index) = self
            .library
            .playlists
            .iter()
            .position(|saved| saved.info.id == info.id)
        {
            self.library.playlists.remove(index);
        } else {
            self.library.playlists.push(crate::library::SavedPlaylist {
                info,
                tracks: self
                    .online_indices
                    .iter()
                    .map(|index| self.library.tracks[*index].path.clone())
                    .collect(),
            });
        }
        self.save();
        cx.notify();
    }

    pub(super) fn sync_saved_playlist(&mut self) {
        let Some(info) = &self.playlist_info else {
            return;
        };

        if let Some(saved) = self
            .library
            .playlists
            .iter_mut()
            .find(|saved| saved.info.id == info.id)
        {
            let mut info = info.clone();
            info.cover = info.cover.or_else(|| saved.info.cover.clone());
            saved.info = info;
            saved.tracks = self
                .online_indices
                .iter()
                .map(|index| self.library.tracks[*index].path.clone())
                .collect();
            self.save();
        }
    }

    pub(super) fn playlist_favorites(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .flex_1()
            .min_h_0()
            .gap_2()
            .pt_4()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("收藏"),
            )
            .child(
                Button::new("favorites")
                    .ghost()
                    .w_full()
                    .h(rems(3.))
                    .flex_shrink_0()
                    .justify_start()
                    .selected(self.page == Page::Favorites)
                    .accessibility_label("我喜欢的")
                    .child(
                        h_flex()
                            .w_full()
                            .gap_3()
                            .child(
                                h_flex()
                                    .size_8()
                                    .flex_shrink_0()
                                    .justify_center()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().muted)
                                    .child(Self::favorite_icon(true).small()),
                            )
                            .child("我喜欢的"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.navigate(Page::Favorites, window, cx)
                    })),
            )
            .child(
                v_flex()
                    .id("favorite-playlists")
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .overflow_y_scroll()
                    .smooth_scroll("favorite-playlists-wheel")
                    .children(self.library.playlists.iter().map(|saved| {
                        let id = saved.info.id;
                        let fallback = || Icon::new(Glyph::ListMusic).small().into_any_element();
                        let cover = div()
                            .size_8()
                            .flex_shrink_0()
                            .rounded(cx.theme().radius)
                            .overflow_hidden()
                            .bg(cx.theme().muted)
                            .child(if let Some(path) = saved.info.cover.clone() {
                                img(path)
                                    .size_full()
                                    .rounded(cx.theme().radius)
                                    .object_fit(ObjectFit::Cover)
                                    .with_loading(fallback)
                                    .with_fallback(fallback)
                                    .into_any_element()
                            } else {
                                h_flex()
                                    .size_full()
                                    .justify_center()
                                    .child(fallback())
                                    .into_any_element()
                            });

                        Button::new(SharedString::from(format!("saved-playlist-{id}")))
                            .ghost()
                            .w_full()
                            .h(rems(3.))
                            .flex_shrink_0()
                            .justify_start()
                            .selected(
                                self.page == Page::Online
                                    && self
                                        .playlist_info
                                        .as_ref()
                                        .is_some_and(|info| info.id == id),
                            )
                            .tooltip(saved.info.name.clone())
                            .accessibility_label(format!("打开收藏歌单 {}", saved.info.name))
                            .child(
                                h_flex().w_full().min_w_0().gap_3().child(cover).child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(saved.info.name.clone()),
                                ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_recommended_playlist(id, window, cx)
                            }))
                    })),
            )
    }
}
