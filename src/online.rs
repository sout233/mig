use crate::library::{Track, storage_directory};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

mod crypto;
mod http;
mod playlist;
mod recommendations;
pub(crate) use recommendations::{RecommendationData, RecommendationPlaylist, recommendations};
mod script;
mod service;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OnlineTrack {
    pub(crate) source: String,
    pub(crate) music_info: Value,
}

#[derive(Clone, Copy, Serialize)]
pub(crate) struct BuiltinSource {
    pub(crate) name: &'static str,
    pub(crate) url: &'static str,
}

pub(crate) const SOURCES: [BuiltinSource; 9] = [
    BuiltinSource {
        name: "Huibq · jsDelivr",
        url: "https://fastly.jsdelivr.net/gh/Huibq/keep-alive/render_api.js",
    },
    BuiltinSource {
        name: "Huibq · ghproxy",
        url: "https://ghproxy.net/raw.githubusercontent.com/pdone/lx-music-source/main/huibq/latest.js",
    },
    BuiltinSource {
        name: "Flower",
        url: "https://ghproxy.net/raw.githubusercontent.com/pdone/lx-music-source/main/flower/latest.js",
    },
    BuiltinSource {
        name: "LX",
        url: "https://ghproxy.net/raw.githubusercontent.com/pdone/lx-music-source/main/lx/latest.js",
    },
    BuiltinSource {
        name: "IKUN · GitHub",
        url: "https://raw.githubusercontent.com/pdone/lx-music-source/main/ikun/latest.js",
    },
    BuiltinSource {
        name: "Grass",
        url: "https://raw.githubusercontent.com/pdone/lx-music-source/main/grass/latest.js",
    },
    BuiltinSource {
        name: "Juhe",
        url: "https://raw.githubusercontent.com/pdone/lx-music-source/main/juhe/latest.js",
    },
    BuiltinSource {
        name: "全豆要",
        url: "https://raw.githubusercontent.com/pdone/lx-music-source/main/qdy/latest.js",
    },
    BuiltinSource {
        name: "IKUN · ghproxy",
        url: "https://ghproxy.net/raw.githubusercontent.com/pdone/lx-music-source/main/ikun/latest.js",
    },
];
#[derive(Deserialize)]
pub(crate) struct SearchResult {
    pub(crate) tracks: Vec<Track>,
    pub(crate) total: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchKind {
    Songs,
    Playlists,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlaylistSummary {
    #[serde(default)]
    pub(crate) cover: Option<PathBuf>,
    #[serde(default)]
    pub(crate) cover_url: Option<String>,
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) creator: String,
    pub(crate) total: usize,
}

pub(crate) struct PlaylistDetail {
    pub(crate) info: PlaylistSummary,
    pub(crate) tracks: Vec<Track>,
}

pub(crate) enum BrowseResult {
    Songs(SearchResult),
    Playlists {
        items: Vec<PlaylistSummary>,
        total: usize,
    },
    Playlist(PlaylistDetail),
}

pub(crate) fn browse(
    query: &str,
    kind: SearchKind,
    page: usize,
    cancel: Arc<AtomicBool>,
) -> Result<BrowseResult, String> {
    if kind == SearchKind::Songs {
        return search(query, page, cancel).map(BrowseResult::Songs);
    }
    playlist::browse(&http::Http::new(cancel), query, kind, page)
}

#[derive(Deserialize)]
pub(crate) struct PreparedTrack {
    pub(crate) file: PathBuf,
    pub(crate) lyric: String,
    pub(crate) translated_lyric: String,
    pub(crate) source: String,
    pub(crate) quality: String,
}

#[derive(Deserialize)]
pub(crate) struct CoverResult {
    pub(crate) id: PathBuf,
    pub(crate) url: Option<String>,
    pub(crate) file: Option<PathBuf>,
}

pub(crate) fn covers(tracks: &[Track]) -> Result<Vec<CoverResult>, String> {
    let http = http::Http::new(Arc::default());
    Ok(service::covers(
        &http,
        tracks,
        &storage_directory().join("online"),
    ))
}

pub(crate) fn search(
    query: &str,
    page: usize,
    cancel: Arc<AtomicBool>,
) -> Result<SearchResult, String> {
    service::search(&http::Http::new(cancel), query, page)
}

pub(crate) fn prepare(track: &Track, cancel: Arc<AtomicBool>) -> Result<PreparedTrack, String> {
    service::prepare(
        &http::Http::new(cancel),
        track,
        &SOURCES,
        &storage_directory().join("online"),
    )
}

pub(crate) fn probe() -> Result<Vec<String>, String> {
    let http = http::Http::new(Arc::default());
    let directory = storage_directory().join("online/sources");
    Ok(std::thread::scope(|scope| {
        let jobs: Vec<_> = SOURCES
            .iter()
            .map(|source| {
                let http = &http;
                let directory = &directory;
                scope.spawn(move || {
                    let result = service::script_for(http, source, directory)
                        .and_then(|raw| script::Source::load(http, &raw));

                    match result {
                        Ok(engine) => format!(
                            "{}：已初始化（{}）",
                            source.name,
                            engine
                                .sources
                                .as_object()
                                .unwrap()
                                .keys()
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(" / ")
                        ),
                        Err(error) => format!("{}：{error}", source.name),
                    }
                })
            })
            .collect();

        jobs.into_iter()
            .map(|job| job.join().unwrap_or_else(|_| "音源检查失败".into()))
            .collect()
    }))
}

#[cfg(test)]
mod tests;
