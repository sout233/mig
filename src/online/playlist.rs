use super::{BrowseResult, PlaylistDetail, PlaylistSummary, SearchKind, http::Http, service};
use serde_json::{Value, json};
use std::collections::HashMap;

const NATIVE: &str = "https://music.163.com";
const FALLBACK: &str = "https://ncm-api.soout.top";

pub(super) fn playlist_id(input: &str) -> Result<Option<u64>, String> {
    let input = input.trim();
    if !input.is_empty() && input.bytes().all(|byte| byte.is_ascii_digit()) {
        return parse_id(input).map(Some);
    }

    let Some(start) = input.find("https://").or_else(|| input.find("http://")) else {
        return Ok(None);
    };

    let address = input[start..].split_whitespace().next().unwrap_or_default();
    let address = address.trim_end_matches([')', '）', '》', ']', '"', '，']);
    let mut url = reqwest::Url::parse(address).map_err(|_| "歌单链接格式不正确")?;
    if !matches!(
        url.host_str(),
        Some("music.163.com" | "m.music.163.com" | "www.music.163.com")
    ) {
        return Err("请输入网易云歌单链接或歌单 ID".into());
    }
    if let Some(fragment) = url
        .fragment()
        .filter(|fragment| fragment.contains("playlist"))
    {
        url = reqwest::Url::parse(&format!("{NATIVE}/{}", fragment.trim_start_matches('/')))
            .map_err(|_| "歌单链接格式不正确")?;
    }

    let segments: Vec<_> = url
        .path()
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if !segments.contains(&"playlist") {
        return Err("链接不是网易云歌单，请使用 playlist 链接".into());
    }

    let id = url
        .query_pairs()
        .find(|(key, _)| key == "id")
        .map(|(_, value)| value.into_owned())
        .or_else(|| {
            segments
                .last()
                .filter(|part| part.bytes().all(|b| b.is_ascii_digit()))
                .map(|part| (*part).to_owned())
        })
        .ok_or("歌单链接中缺少 ID")?;

    parse_id(&id).map(Some)
}

fn parse_id(id: &str) -> Result<u64, String> {
    id.parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| "歌单 ID 必须是有效的正整数".into())
}

fn summary(value: &Value) -> PlaylistSummary {
    PlaylistSummary {
        cover: None,
        cover_url: value
            .get("coverImgUrl")
            .or_else(|| value.get("picUrl"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        id: value["id"].as_u64().unwrap_or(0),
        name: value["name"].as_str().unwrap_or("未命名歌单").into(),
        creator: value["creator"]["nickname"]
            .as_str()
            .unwrap_or_default()
            .into(),
        total: value["trackCount"].as_u64().unwrap_or(0) as usize,
    }
}

pub(super) fn get(
    http: &Http,
    base: &str,
    path: &str,
    params: &[(&str, String)],
) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(&format!("{base}{path}")).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .extend_pairs(params.iter().map(|(key, value)| (*key, value.as_str())));
    let body = http
        .fetch(
            url.as_str(),
            &json!({"timeout":12000,"maxBytes":16*1024*1024,
        "headers":{"Referer":"https://music.163.com/"}}),
        )?
        .body;

    if body["code"] != 200 {
        return Err(format!(
            "接口返回 {}：{}",
            body["code"],
            body["message"].as_str().unwrap_or("请求失败")
        ));
    }
    Ok(body)
}

pub(super) fn browse(
    http: &Http,
    query: &str,
    kind: SearchKind,
    page: usize,
) -> Result<BrowseResult, String> {
    http.check()?;
    if kind == SearchKind::Songs {
        return service::search(http, query, page).map(BrowseResult::Songs);
    }
    if let Some(id) = playlist_id(query)? {
        return load_with(http, id, NATIVE, FALLBACK).map(BrowseResult::Playlist);
    }

    let params = [
        ("s", query.to_owned()),
        ("type", "1000".into()),
        ("limit", "30".into()),
        ("offset", (page.saturating_sub(1) * 30).to_string()),
    ];

    let body = get(http, NATIVE, "/api/search/get/web", &params).or_else(|_| {
        http.check()?;
        get(
            http,
            FALLBACK,
            "/search",
            &[
                ("keywords", query.into()),
                ("type", "1000".into()),
                ("limit", "30".into()),
                ("offset", (page.saturating_sub(1) * 30).to_string()),
            ],
        )
    })?;

    let result = &body["result"];
    let items = result["playlists"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(summary)
                .filter(|item| item.id > 0)
                .collect()
        })
        .unwrap_or_default();

    if !result["playlists"].is_array() && result["playlistCount"] != 0 {
        return Err("歌单搜索结果格式异常".into());
    }
    Ok(BrowseResult::Playlists {
        items,
        total: result["playlistCount"].as_u64().unwrap_or(0) as usize,
    })
}

pub(super) fn load_with(
    http: &Http,
    id: u64,
    native: &str,
    fallback: &str,
) -> Result<PlaylistDetail, String> {
    let params = [
        ("id", id.to_string()),
        ("n", "100000".into()),
        ("s", "0".into()),
    ];

    let body = get(http, native, "/api/v6/playlist/detail", &params)
        .and_then(|body| {
            if body["playlist"]["trackIds"].is_array() {
                Ok(body)
            } else {
                Err("歌单详情缺少完整歌曲 ID".into())
            }
        })
        .or_else(|_| {
            http.check()?;
            get(http, fallback, "/playlist/detail", &params)
        })?;

    let playlist = &body["playlist"];
    if !playlist.is_object() || playlist["id"].as_u64() != Some(id) {
        return Err("歌单不存在或不可访问".into());
    }

    let ids: Vec<u64> = playlist["trackIds"]
        .as_array()
        .ok_or("歌单未返回完整歌曲 ID")?
        .iter()
        .filter_map(|item| item["id"].as_u64())
        .collect();

    let mut songs: HashMap<u64, Value> = playlist["tracks"]
        .as_array()
        .map(|songs| {
            songs
                .iter()
                .filter_map(|song| song["id"].as_u64().map(|id| (id, song.clone())))
                .collect()
        })
        .unwrap_or_default();

    let missing: Vec<_> = ids
        .iter()
        .filter(|id| !songs.contains_key(id))
        .copied()
        .collect();

    for chunk in missing.chunks(100) {
        http.check()?;

        let details = get(
            http,
            native,
            "/api/song/detail",
            &[("ids", json!(chunk).to_string())],
        )
        .and_then(valid_songs)
        .or_else(|_| {
            http.check()?;
            get(
                http,
                fallback,
                "/song/detail",
                &[(
                    "ids",
                    chunk
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                )],
            )
            .and_then(valid_songs)
        })?;

        for song in details {
            if let Some(id) = song["id"].as_u64() {
                songs.insert(id, song);
            }
        }
    }
    let mut info = summary(playlist);
    info.total = ids.len();
    if let Some(url) = &info.cover_url {
        info.cover = service::cache_cover(
            http,
            url,
            &crate::library::storage_directory().join("online/covers"),
        )
        .ok();
    }

    let tracks = ids
        .iter()
        .filter_map(|id| songs.get(id))
        .map(service::netease_track)
        .collect();

    Ok(PlaylistDetail { info, tracks })
}

fn valid_songs(body: Value) -> Result<Vec<Value>, String> {
    body["songs"]
        .as_array()
        .cloned()
        .ok_or_else(|| "歌曲详情格式异常".into())
}
