mod content;
mod cover;
mod expanded;
mod local;
mod lyrics_geometry;
mod lyrics_visual;
mod motion;
mod navigation;
mod online;
mod panel;
mod playlists;
mod progress;
mod recommendations;
mod shell;
mod smooth_scroll;
mod tips;
mod title;
#[cfg(feature = "ui-verify")]
pub(crate) mod verify;

use crate::{
    library::{Library, LyricLine, Track},
    player::{Command, Event, PlayMode, Player},
};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::{
    input::{InputEvent, InputState},
    select::{SelectEvent, SelectState},
    slider::{SliderEvent, SliderState},
    *,
};
use gpui_kit::*;
use smooth_scroll::ScrollTarget as _;
use std::{path::PathBuf, time::Duration};

actions!(
    music,
    [
        ImportMusic,
        NavigateBack,
        NavigateForward,
        CloseLyrics,
        SearchMusic,
        TogglePlayback,
        NextTrack,
        PreviousTrack
    ]
);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page {
    Library,
    Local,
    Favorites,
    Recent,
    Settings,
    Online,
    Sources,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Library => "推荐",
            Self::Local => "本地音乐",
            Self::Favorites => "我喜欢的",
            Self::Recent => "最近播放",
            Self::Settings => "设置",
            Self::Online => "搜索",
            Self::Sources => "内置音源",
        }
    }
}

pub(crate) struct MigApp {
    library: Library,
    page: Page,
    search: Entity<InputState>,
    query: String,
    visible: Vec<usize>,
    player: Player,
    current: Option<Track>,
    title_started: std::time::Instant,
    page_transition: u64,
    tip: &'static str,
    navigation_back: Vec<navigation::NavigationEntry>,
    navigation_forward: Vec<navigation::NavigationEntry>,
    queue: Vec<Track>,
    lyrics: Vec<LyricLine>,
    lyrics_bilingual: bool,
    position: f64,
    duration: f64,
    playing: bool,
    opening: bool,
    mode: PlayMode,
    progress: Entity<SliderState>,
    volume: Entity<SliderState>,
    lyrics_spacing: Entity<SliderState>,
    seeking: bool,
    panel: Option<bool>, // true: queue, false: lyrics
    queue_popup: Option<panel::QueuePopup>,
    expanded_lyrics: bool,
    bottom_cover_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    expanded_cover_bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    cover_morph: Option<expanded::ExpandedTransition>,
    sidebar_split: Entity<ResizableState>,
    panel_split: Entity<ResizableState>,
    lyrics_scroll: UniformListScrollHandle,
    lyrics_active: Option<usize>,
    lyrics_follow: bool,
    lyrics_needs_scroll: bool,
    lyrics_layout: Option<(Pixels, Pixels)>,
    lyrics_geometry: lyrics_geometry::LyricsGeometry,
    bottom_cover_fade: Option<(f32, f32, std::time::Instant)>,
    lyrics_scroll_motion: Option<(Pixels, Pixels, std::time::Instant)>,
    loading: bool,
    importing: bool,
    local_import_status: Option<String>,
    error: Option<String>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
    _timer: Task<()>,
    online_indices: Vec<usize>,
    online_total: usize,
    online_page: usize,
    online_query: String,
    search_kind: Entity<SelectState<Vec<&'static str>>>,
    online_kind: crate::online::SearchKind,
    playlists: Vec<crate::online::PlaylistSummary>,
    playlist_info: Option<crate::online::PlaylistSummary>,
    online_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    searching: bool,
    source_status: Vec<String>,
    probing: bool,
    playback_status: String,
    recommendations: Option<crate::online::RecommendationData>,
    recommendations_loading: bool,
    recommendations_error: Option<String>,
    recommendations_requested: bool,
    cover_busy: bool,
    cover_requested: std::collections::HashSet<PathBuf>,
    theme_cover: Option<PathBuf>,
    theme_accent: Option<u32>,
    cover_colors: std::collections::HashMap<PathBuf, Option<u32>>,
}

impl MigApp {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("搜索歌曲、歌手、专辑"));
        let progress = cx.new(|_| SliderState::new().min(0.).max(100.).step(0.1));
        let volume = cx.new(|_| SliderState::new().min(0.).max(100.).default_value(65.));

        let lyrics_spacing = cx.new(|_| {
            SliderState::new()
                .min(80.)
                .max(200.)
                .step(5.)
                .default_value(100.)
        });

        let search_kind = cx
            .new(|cx| SelectState::new(vec!["歌曲", "歌单"], Some(IndexPath::new(0)), window, cx));

        let search_focus = search.read(cx).focus_handle(cx);
        let subscriptions = vec![
            cx.on_focus_in(&search_focus, window, |_, _, cx| cx.notify()),
            cx.on_focus_out(&search_focus, window, |_, _, _, cx| cx.notify()),
            cx.subscribe_in(&search_kind, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    let kind = if *value == "歌单" {
                        crate::online::SearchKind::Playlists
                    } else {
                        crate::online::SearchKind::Songs
                    };

                    if this.online_kind != kind {
                        if this.page == Page::Online {
                            this.remember_navigation();
                        }
                        this.cancel_search();
                        this.online_kind = kind;
                        this.online_query.clear();
                        this.online_indices.clear();
                        this.playlists.clear();
                        this.playlist_info = None;
                        this.online_total = 0;
                        this.online_page = 1;
                        this.error = None;
                        this.update_search_placeholder(window, cx);
                        this.refresh_filter();
                        cx.notify();
                    }
                }
            }),
            cx.subscribe_in(&search, window, |this, state, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    if this.page == Page::Online {
                        this.cancel_search();
                    }
                    this.query = state.read(cx).value().to_string();
                    this.refresh_filter();
                    cx.notify();
                }
                if matches!(event, InputEvent::PressEnter { .. }) {
                    if this.page == Page::Library && !this.query.trim().is_empty() {
                        let query = this.query.clone();
                        this.navigate(Page::Online, window, cx);
                        this.query = query;
                        this.search.update(cx, |input, cx| {
                            input.set_value(this.query.clone(), window, cx)
                        });
                    }
                    if this.page == Page::Online {
                        this.search_online(1, cx);
                    }
                }
            }),
            cx.subscribe_in(&progress, window, |this, _, event, _, cx| {
                match event {
                    SliderEvent::Change(value) => {
                        this.seeking = true;
                        this.position = this.duration * f64::from(value.start()) / 100.;
                    }
                    SliderEvent::Release(value) => {
                        this.player.send(Command::Seek(
                            this.duration * f64::from(value.start()) / 100.,
                        ));
                        this.seeking = false;
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&lyrics_spacing, |this, _, event, cx| match event {
                SliderEvent::Change(value) | SliderEvent::Release(value) => {
                    this.library.lyrics_spacing = Some(value.start().clamp(80., 200.));
                    this.lyrics_needs_scroll = true;
                    this.lyrics_layout = None;
                    this.lyrics_scroll_motion = None;
                    if matches!(event, SliderEvent::Release(_)) {
                        this.save();
                    }
                    cx.notify();
                }
            }),
            cx.subscribe(&volume, |this, _, event, cx| {
                if let SliderEvent::Change(value) = event {
                    this.player.send(Command::Volume(value.start() / 100.));
                    cx.notify();
                }
            }),
        ];

        let timer = cx.spawn_in(window, async move |this, cx| {
            loop {
                let animating = this
                    .read_with(cx, |this, _| this.cover_morph_active())
                    .unwrap_or(false);

                cx.background_executor()
                    .timer(Duration::from_millis(if animating { 16 } else { 250 }))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| {
                        this.receive_events(window, cx);
                        if this.cover_morph.is_some() {
                            if !this.cover_morph_active() {
                                if this
                                    .cover_morph
                                    .as_ref()
                                    .is_some_and(|motion| motion.closing)
                                {
                                    this.expanded_lyrics = false;
                                }
                                this.cover_morph = None;
                            }
                            cx.notify();
                        }
                        this.load_covers(cx);
                        if !this.loading
                            && this.page == Page::Library
                            && !this.recommendations_requested
                        {
                            this.load_recommendations(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async {
                    let mut library = Library::load()?;
                    for track in &mut library.tracks {
                        crate::local_metadata::enrich(track);
                        if track.cover.as_ref().is_some_and(|path| !path.is_file()) {
                            track.cover = None;
                        }
                    }
                    for playlist in &mut library.playlists {
                        if playlist
                            .info
                            .cover
                            .as_ref()
                            .is_some_and(|path| !path.is_file())
                        {
                            playlist.info.cover = None;
                        }
                    }
                    Ok::<_, String>(library)
                })
                .await;

            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(library) => {
                        this.library = library;
                        let spacing = this.library.lyrics_spacing_percent();
                        this.lyrics_spacing
                            .update(cx, |state, cx| state.set_value(spacing, window, cx));
                        this.sidebar_split.update(cx, |state, _| state.clear());
                        this.panel_split.update(cx, |state, _| state.clear());
                        this.save();
                        crate::theme::apply(this.library.dark, cx);
                        this.refresh_filter();
                    }
                    Err(error) => this.error = Some(error),
                }
                this.loading = false;
                cx.notify();
            });
        })
        .detach();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            library: Library::default(),
            page: Page::Library,
            search,
            query: String::new(),
            visible: Vec::new(),
            player: Player::new(),
            current: None,
            title_started: std::time::Instant::now(),
            page_transition: 0,
            tip: tips::pick(None),
            navigation_back: Vec::new(),
            navigation_forward: Vec::new(),
            queue: Vec::new(),
            lyrics: Vec::new(),
            lyrics_bilingual: false,
            position: 0.,
            duration: 0.,
            playing: false,
            opening: false,
            mode: PlayMode::Sequential,
            progress,
            volume,
            lyrics_spacing,
            seeking: false,
            panel: None,
            queue_popup: None,
            expanded_lyrics: false,
            bottom_cover_bounds: Default::default(),
            expanded_cover_bounds: Default::default(),
            cover_morph: None,
            sidebar_split: cx.new(|_| ResizableState::default()),
            panel_split: cx.new(|_| ResizableState::default()),
            lyrics_scroll: UniformListScrollHandle::new(),
            lyrics_active: None,
            lyrics_follow: true,
            lyrics_needs_scroll: true,
            lyrics_layout: None,
            lyrics_geometry: lyrics_geometry::LyricsGeometry::default(),
            bottom_cover_fade: None,
            lyrics_scroll_motion: None,
            loading: true,
            importing: false,
            local_import_status: None,
            error: None,
            focus,
            scroll: UniformListScrollHandle::new(),
            _subscriptions: subscriptions,
            _timer: timer,
            online_indices: Vec::new(),
            online_total: 0,
            online_page: 1,
            online_query: String::new(),
            search_kind,
            online_kind: crate::online::SearchKind::Songs,
            playlists: Vec::new(),
            playlist_info: None,
            online_cancel: None,
            searching: false,
            source_status: Vec::new(),
            probing: false,
            playback_status: "idk".into(),
            recommendations: None,
            recommendations_loading: false,
            recommendations_error: None,
            recommendations_requested: false,
            cover_busy: false,
            cover_requested: std::collections::HashSet::new(),
            theme_cover: None,
            theme_accent: None,
            cover_colors: std::collections::HashMap::new(),
        }
    }

    fn refresh_filter(&mut self) {
        if self.page == Page::Online {
            self.visible = self.online_indices.clone();
            return;
        }
        self.visible = self
            .library
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                track.matches(&self.query)
                    && match self.page {
                        Page::Local => track.online.is_none(),
                        Page::Favorites => track.favorite,
                        Page::Recent => self.library.recent.contains(&track.path),
                        _ => true,
                    }
            })
            .map(|(ix, _)| ix)
            .collect();
        if self.page == Page::Recent {
            self.visible.sort_by_key(|ix| {
                self.library
                    .recent
                    .iter()
                    .position(|path| *path == self.library.tracks[*ix].path)
            });
        }
    }

    fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != page {
            self.remember_navigation();
        }
        self.cancel_search();
        self.cover_requested.clear();
        if self.page != page {
            self.page_transition = self.page_transition.wrapping_add(1);
        }
        self.tip = tips::pick(Some(self.tip));
        self.page = page;
        self.update_search_placeholder(window, cx);
        self.query.clear();
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        if page == Page::Online {
            self.query = self.online_query.clone();
            self.search.update(cx, |input, cx| {
                input.set_value(self.query.clone(), window, cx)
            });
        }
        self.refresh_filter();
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn play_visible(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.queue = self
            .visible
            .iter()
            .map(|ix| self.library.tracks[*ix].clone())
            .collect();
        if ix < self.queue.len() {
            self.opening = true;
            self.error = None;
            self.player.send(Command::Play(self.queue.clone(), ix));
            cx.notify();
        }
    }

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if self.current.is_some() || self.opening {
            self.player.send(Command::Toggle);
        } else {
            self.play_visible(0, cx);
        }
    }

    fn favorite(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Some(track) = self
            .library
            .tracks
            .iter_mut()
            .find(|track| track.path == path)
        {
            track.favorite = !track.favorite;
        }
        if let Some(current) = &mut self.current
            && let Some(track) = self
                .library
                .tracks
                .iter()
                .find(|track| track.path == current.path)
        {
            current.favorite = track.favorite;
        }
        for queued in &mut self.queue {
            if let Some(track) = self
                .library
                .tracks
                .iter()
                .find(|track| track.path == queued.path)
            {
                queued.favorite = track.favorite;
            }
        }
        self.refresh_filter();
        self.save();
        cx.notify();
    }

    fn save(&self) {
        self.player.send(Command::Save(self.library.clone()));
    }

    fn receive_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = self.player.drain();
        if events.is_empty() {
            return;
        }
        for event in events {
            match event {
                Event::Started(track, duration, lyrics) => {
                    let mut track = *track;
                    if track.online.is_none() {
                        self.playback_status = "本地播放".into();
                    }
                    self.library.record_play(&track.path);
                    if let Some(source) = self
                        .library
                        .tracks
                        .iter_mut()
                        .find(|source| source.path == track.path)
                    {
                        source.duration = (duration > 0.).then_some(duration);
                        track.favorite = source.favorite;
                        if track.online.is_none() {
                            source.title = track.title.clone();
                            source.artist = track.artist.clone();
                            source.album = track.album.clone();
                            source.cover = track.cover.clone().or_else(|| source.cover.clone());
                        }
                        track.cover = source.cover.clone();
                    }
                    self.current = Some(track);
                    self.title_started = std::time::Instant::now();
                    self.refresh_cover_theme(cx);
                    self.duration = duration;
                    self.position = 0.;
                    self.lyrics_bilingual = lyrics.iter().any(|line| line.translation.is_some());
                    self.lyrics = lyrics;
                    self.lyrics_geometry = lyrics_geometry::LyricsGeometry::default();
                    self.lyrics_active = None;
                    if self
                        .lyrics
                        .first()
                        .is_some_and(|line| !line.seconds.is_finite())
                    {
                        self.lyrics_scroll
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(point(px(0.), px(0.)));
                    }
                    self.lyrics_needs_scroll = true;
                    self.lyrics_follow = true;
                    self.lyrics_scroll_motion = None;
                    self.opening = false;
                    self.refresh_filter();
                    self.save();
                }
                Event::Position(position) if !self.seeking => self.position = position,
                Event::Stopped => {
                    self.current = None;
                    self.playing = false;
                    self.opening = false;
                    self.position = 0.;
                    self.duration = 0.;
                    self.lyrics.clear();
                    self.lyrics_geometry = lyrics_geometry::LyricsGeometry::default();
                    self.lyrics_active = None;
                    self.lyrics_scroll_motion = None;
                    self.expanded_lyrics = false;
                    self.cover_morph = None;
                    self.progress
                        .update(cx, |state, cx| state.set_value(0., window, cx));
                    self.refresh_cover_theme(cx);
                }
                Event::Playing(playing) => {
                    if self.opening && !playing {
                        self.playback_status = "加载已取消，可重新播放".into();
                    }
                    self.playing = playing;
                    self.opening = false;
                }
                Event::Error(error) | Event::SaveError(error) => {
                    if self.opening {
                        self.playback_status = "加载失败，请重试或选择其他歌曲".into();
                    }
                    self.error = Some(error);
                    self.opening = false;
                }
                Event::Loading(status) => {
                    self.playback_status = status;
                    self.opening = true;
                }
                Event::Source(status) => self.playback_status = format!("当前音源：{status}"),
                _ => {}
            }
        }
        if !self.seeking {
            let value = if self.duration > 0. {
                (self.position / self.duration * 100.) as f32
            } else {
                0.
            };

            self.progress
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        cx.notify();
    }
}
