use super::{
    http::{Http, write_cache},
    playlist, service,
};
use crate::library::{Track, storage_directory};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RecommendationPlaylist {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) cover: Option<PathBuf>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct RecommendationData {
    pub(crate) playlists: Vec<RecommendationPlaylist>,
    pub(crate) tracks: Vec<Track>,
    pub(crate) notice: Option<String>,
}

pub(crate) fn recommendations() -> Result<RecommendationData, String> {
    let http = Http::new(Arc::new(AtomicBool::new(false)));
    let directory = storage_directory().join("online");
    let cache = directory.join("recommendations.json");
    let mut data = RecommendationData::default();

    let (lists, songs) = std::thread::scope(|scope| {
        let lists = scope.spawn(|| {
            playlist::get(
                &http,
                "https://music.163.com",
                "/api/playlist/list",
                &[
                    ("cat", "全部".into()),
                    ("order", "hot".into()),
                    ("limit", "10".into()),
                    ("offset", "0".into()),
                ],
            )
            .or_else(|_| {
                playlist::get(
                    &http,
                    "https://ncm-api.soout.top",
                    "/personalized",
                    &[("limit", "10".into())],
                )
            })
        });

        let songs = scope.spawn(|| {
            playlist::get(
                &http,
                "https://music.163.com",
                "/api/v1/discovery/new/songs",
                &[],
            )
            .or_else(|_| {
                playlist::get(
                    &http,
                    "https://ncm-api.soout.top",
                    "/personalized/newsong",
                    &[("limit", "10".into())],
                )
            })
        });

        (
            lists.join().unwrap_or_else(|_| Err("歌单请求中断".into())),
            songs.join().unwrap_or_else(|_| Err("新歌请求中断".into())),
        )
    });

    let mut errors = Vec::new();
    match lists {
        Ok(body) => {
            let items = body
                .get("playlists")
                .or_else(|| body.get("result"))
                .and_then(Value::as_array)
                .ok_or("推荐歌单格式异常")?;

            let covers = directory.join("covers");
            for batch in items.chunks(4) {
                let cards = std::thread::scope(|scope| {
                    let tasks: Vec<_> = batch
                        .iter()
                        .map(|item| {
                            let http = &http;
                            let covers = &covers;
                            scope.spawn(move || RecommendationPlaylist {
                                id: item["id"].as_u64().unwrap_or(0),
                                name: item["name"].as_str().unwrap_or("精选歌单").into(),
                                description: item["description"]
                                    .as_str()
                                    .unwrap_or("网易云精选歌单")
                                    .lines()
                                    .next()
                                    .unwrap_or_default()
                                    .into(),
                                cover: item
                                    .get("coverImgUrl")
                                    .or_else(|| item.get("picUrl"))
                                    .and_then(Value::as_str)
                                    .and_then(|url| service::cache_cover(http, url, covers).ok()),
                            })
                        })
                        .collect();

                    tasks
                        .into_iter()
                        .filter_map(|task| task.join().ok())
                        .filter(|card| card.id > 0)
                        .collect::<Vec<_>>()
                });

                data.playlists.extend(cards);
            }
        }
        Err(error) => errors.push(error),
    }
    match songs {
        Ok(body) => {
            if let Some(items) = body
                .get("data")
                .or_else(|| body.get("result"))
                .and_then(Value::as_array)
            {
                data.tracks = items
                    .iter()
                    .take(10)
                    .map(|item| service::netease_track(item.get("song").unwrap_or(item)))
                    .filter(|track| !track.title.is_empty())
                    .collect();
                let covers = service::covers(&http, &data.tracks, &directory);
                for track in &mut data.tracks {
                    track.cover = covers
                        .iter()
                        .find(|cover| cover.id == track.path)
                        .and_then(|cover| cover.file.clone());
                }
            } else {
                errors.push("推荐新歌格式异常".into());
            }
        }
        Err(error) => errors.push(error),
    }
    if data.playlists.is_empty() && data.tracks.is_empty() {
        let cached = std::fs::read_to_string(&cache)
            .ok()
            .and_then(|json| serde_json::from_str::<RecommendationData>(&json).ok());

        if let Some(mut cached) = cached {
            cached.notice = Some("暂时无法更新，正在显示上次的推荐".into());
            for card in &mut cached.playlists {
                if card.cover.as_ref().is_some_and(|path| !path.is_file()) {
                    card.cover = None;
                }
            }
            for track in &mut cached.tracks {
                if track.cover.as_ref().is_some_and(|path| !path.is_file()) {
                    track.cover = None;
                }
            }
            return Ok(cached);
        }
        return Err("暂时无法获取推荐，请稍后重试".into());
    }
    if !errors.is_empty() {
        data.notice = Some("部分推荐暂时无法更新，可稍后刷新".into());
    }
    if let Ok(json) = serde_json::to_vec(&data) {
        let _ = write_cache(&cache, &json);
    }
    Ok(data)
}
