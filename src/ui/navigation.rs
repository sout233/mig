use super::*;

#[derive(Clone)]
pub(super) struct NavigationEntry {
    page: Page,
    query: String,
    tip: &'static str,
    online_kind: crate::online::SearchKind,
    online_indices: Vec<usize>,
    online_total: usize,
    online_page: usize,
    online_query: String,
    playlists: Vec<crate::online::PlaylistSummary>,
    playlist_info: Option<crate::online::PlaylistSummary>,
}

impl MigApp {
    fn navigation_entry(&self) -> NavigationEntry {
        NavigationEntry {
            page: self.page,
            query: self.query.clone(),
            tip: self.tip,
            online_kind: self.online_kind,
            online_indices: self.online_indices.clone(),
            online_total: self.online_total,
            online_page: self.online_page,
            online_query: self.online_query.clone(),
            playlists: self.playlists.clone(),
            playlist_info: self.playlist_info.clone(),
        }
    }

    pub(super) fn remember_navigation(&mut self) {
        self.navigation_back.push(self.navigation_entry());
        if self.navigation_back.len() > 50 {
            self.navigation_back.remove(0);
        }
        self.navigation_forward.clear();
    }

    pub(super) fn browse_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.expanded_lyrics {
            return;
        }

        let target = if forward {
            self.navigation_forward.pop()
        } else {
            self.navigation_back.pop()
        };

        let Some(target) = target else { return };
        let current = self.navigation_entry();
        if forward {
            self.navigation_back.push(current);
        } else {
            self.navigation_forward.push(current);
        }
        self.cancel_search();
        self.cover_requested.clear();
        self.page = target.page;
        self.query = target.query;
        self.tip = target.tip;
        self.online_kind = target.online_kind;
        self.online_indices = target.online_indices;
        self.online_total = target.online_total;
        self.online_page = target.online_page;
        self.online_query = target.online_query;
        self.playlists = target.playlists;
        self.playlist_info = target.playlist_info;
        self.page_transition = self.page_transition.wrapping_add(1);
        self.error = None;
        self.search_kind.update(cx, |state, cx| {
            state.set_selected_value(
                &if self.online_kind == crate::online::SearchKind::Playlists {
                    "歌单"
                } else {
                    "歌曲"
                },
                window,
                cx,
            )
        });
        self.update_search_placeholder(window, cx);
        self.search.update(cx, |state, cx| {
            state.set_value(self.query.clone(), window, cx)
        });
        self.refresh_filter();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        window.focus(&self.focus, cx);
        cx.notify();
    }
}
