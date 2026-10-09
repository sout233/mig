use super::http::{Http, deadline, key, text, write_cache};
use super::{BuiltinSource, CoverResult, OnlineTrack, PreparedTrack, SearchResult};
use crate::library::Track;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) fn search(http: &Http, query: &str, page: usize) -> Result<SearchResult, String> {
    let response = http.fetch("https://music.163.com/api/search/get/web", &json!({
        "method": "POST", "timeout": 15000, "headers": {"Referer": "https://music.163.com/"},
        "form": {"s": query, "type": 1, "limit": 30, "offset": page.saturating_sub(1) * 30, "total": "true"}
    }))?;

    parse_search(http, response.body)
}

pub(super) fn parse_search(http: &Http, body: Value) -> Result<SearchResult, String> {
    let result = &body["result"];
    if body["code"] != 200
        || result.is_null()
        || (!result["songs"].is_array() && result["songCount"] != 0)
    {
        return Err(format!("网易云搜索返回异常（{}）", body["code"]));
    }

    let mut tracks = result["songs"]
        .as_array()
        .map(|songs| songs.iter().map(netease_track).collect::<Vec<_>>())
        .unwrap_or_default();

    hydrate_covers(http, &mut tracks);
    Ok(SearchResult {
        tracks,
        total: result["songCount"].as_u64().unwrap_or(0) as usize,
    })
}

pub(super) fn netease_track(song: &Value) -> Track {
    let album = song
        .get("al")
        .or_else(|| song.get("album"))
        .unwrap_or(&Value::Null);

    let artist = song
        .get("ar")
        .or_else(|| song.get("artists"))
        .and_then(Value::as_array)
        .map(|artists| {
            artists
                .iter()
                .filter_map(|artist| artist["name"].as_str())
                .collect::<Vec<_>>()
                .join("、")
        })
        .unwrap_or_default();

    let duration = song
        .get("dt")
        .or_else(|| song.get("duration"))
        .and_then(Value::as_f64)
        .unwrap_or(0.)
        / 1000.;

    Track {
        path: PathBuf::from(format!("online/wy/{}", text(&song["id"]))),
        title: song["name"].as_str().unwrap_or_default().into(),
        artist: artist.clone(),
        album: album["name"].as_str().unwrap_or_default().into(),
        duration: (duration > 0.).then_some(duration),
        favorite: false,
        cover: None,
        online: Some(OnlineTrack {
            source: "wy".into(),
            music_info: json!({
                "source":"wy", "songmid":song["id"], "name":song["name"], "singer":artist,
                "albumName":album["name"].as_str().unwrap_or_default(), "albumId":album["id"], "img":album["picUrl"],
                "types":[{"type":"128k"}], "_types":{"128k":{}}, "typeUrl":{}
            }),
        }),
    }
}

fn hydrate_covers(http: &Http, tracks: &mut [Track]) {
    let ids: Vec<_> = tracks
        .iter()
        .filter_map(|track| track.online.as_ref())
        .filter(|online| online.source == "wy" && online.music_info["img"].as_str().is_none())
        .map(|online| online.music_info["songmid"].clone())
        .collect();

    if ids.is_empty() {
        return;
    }

    let Ok(mut url) = reqwest::Url::parse("https://music.163.com/api/song/detail") else {
        return;
    };

    url.query_pairs_mut()
        .append_pair("ids", &json!(ids).to_string());
    let Ok(response) = http.fetch(
        url.as_str(),
        &json!({"timeout":4000,"headers":{"Referer":"https://music.163.com/"}}),
    ) else {
        return;
    };

    if let Some(songs) = response.body["songs"].as_array() {
        for track in tracks {
            if let Some(online) = &mut track.online
                && online.source == "wy"
            {
                let id = text(&online.music_info["songmid"]);
                if let Some(song) = songs.iter().find(|song| text(&song["id"]) == id) {
                    let album = song
                        .get("al")
                        .or_else(|| song.get("album"))
                        .unwrap_or(&Value::Null);

                    if let Some(url) = album["picUrl"].as_str() {
                        online.music_info["img"] = json!(url);
                    }
                }
            }
        }
    }
}

fn cover(http: &Http, track: &Track, directory: &Path) -> CoverResult {
    let mut url = track
        .online
        .as_ref()
        .and_then(|online| online.music_info["img"].as_str())
        .map(str::to_owned);

    let file = (|| {
        if let Some(online) = &track.online
            && url.is_none()
            && online.source == "kw"
        {
            let mut endpoint = reqwest::Url::parse("https://artistpicserver.kuwo.cn/pic.web")
                .map_err(|e| e.to_string())?;

            endpoint.query_pairs_mut().extend_pairs([
                ("corp", "kuwo"),
                ("type", "rid_pic"),
                ("pictype", "500"),
                ("size", "500"),
                ("rid", &text(&online.music_info["songmid"])),
            ]);
            url = Some(
                String::from_utf8(http.fetch(endpoint.as_str(), &json!({"timeout":4000}))?.raw)
                    .map_err(|e| e.to_string())?
                    .trim()
                    .to_owned(),
            );
        }
        cache_cover(http, url.as_deref().ok_or("暂无封面")?, directory)
    })()
    .ok();

    CoverResult {
        id: track.path.clone(),
        url,
        file,
    }
}

pub(super) fn cache_cover(http: &Http, url: &str, directory: &Path) -> Result<PathBuf, String> {
    let mut address = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if !matches!(address.scheme(), "http" | "https") {
        return Err("暂无封面".into());
    }
    let filename = directory.join(format!("{}.image", key(url)));
    // Songs from the same album can share the cover download
    type Locks = std::sync::Mutex<HashMap<PathBuf, std::sync::Weak<std::sync::Mutex<()>>>>;
    static LOCKS: std::sync::OnceLock<Locks> = std::sync::OnceLock::new();

    let lock = {
        let mut locks = LOCKS
            .get_or_init(Default::default)
            .lock()
            .map_err(|e| e.to_string())?;

        if locks.len() > 256 {
            locks.retain(|_, value| value.strong_count() > 0);
        }

        let lock = locks
            .get(&filename)
            .and_then(std::sync::Weak::upgrade)
            .unwrap_or_default();

        locks.insert(filename.clone(), std::sync::Arc::downgrade(&lock));
        lock
    };

    let _guard = lock.lock().map_err(|e| e.to_string())?;
    if std::fs::metadata(&filename).is_ok_and(|info| info.len() > 32) {
        return Ok(filename);
    }
    if address
        .host_str()
        .is_some_and(|host| host.ends_with(".music.126.net"))
    {
        let _ = address.set_scheme("https");
        address.query_pairs_mut().append_pair("param", "256y256");
    }
    let response = http.fetch(address.as_str(), &json!({"timeout":6000,"maxBytes":2*1024*1024,"headers":{"Referer":"https://music.163.com/"}}))?;
    let raw = &response.raw;
    if !(raw.starts_with(b"\x89PNG\r\n\x1a\n")
        || raw.starts_with(b"\xff\xd8\xff")
        || raw.starts_with(b"RIFF") && raw.get(8..12) == Some(b"WEBP"))
    {
        return Err("封面地址未返回图片".into());
    }
    write_cache(&filename, raw)?;
    Ok(filename)
}

pub(super) fn covers(http: &Http, tracks: &[Track], directory: &Path) -> Vec<CoverResult> {
    let mut tracks = tracks.to_vec();
    hydrate_covers(http, &mut tracks);
    let directory = directory.join("covers");
    let mut results = Vec::new();
    let mut cached = HashMap::new();
    for chunk in tracks.chunks(4) {
        let batch = std::thread::scope(|scope| {
            let jobs: Vec<_> = chunk
                .iter()
                .map(|track| {
                    let url = track
                        .online
                        .as_ref()
                        .and_then(|online| online.music_info["img"].as_str());

                    let previous = url.and_then(|url| cached.get(url)).cloned();
                    let directory = &directory;
                    scope.spawn(move || {
                        previous
                            .map(|file| CoverResult {
                                id: track.path.clone(),
                                url: url.map(str::to_owned),
                                file,
                            })
                            .unwrap_or_else(|| cover(http, track, directory))
                    })
                })
                .collect();

            jobs.into_iter()
                .filter_map(|job| job.join().ok())
                .collect::<Vec<_>>()
        });

        for cover in &batch {
            if let Some(url) = &cover.url {
                cached.insert(url.clone(), cover.file.clone());
            }
        }
        results.extend(batch);
    }
    results
}

pub(super) fn script_for(
    http: &Http,
    source: &BuiltinSource,
    directory: &Path,
) -> Result<String, String> {
    let filename = directory.join(format!("{}.js", key(source.url)));
    let cached = std::fs::read_to_string(&filename).ok(); // Rust validates UTF-8.
    if std::fs::metadata(&filename)
        .and_then(|info| info.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(86400))
        && let Some(script) = &cached
    {
        return Ok(script.clone());
    }

    let result = (|| {
        let response = http.fetch(source.url, &json!({"timeout":8000}))?;
        let script = String::from_utf8(response.raw).map_err(|e| e.to_string())?;
        if script.trim_start().starts_with('<') || script.len() < 100 {
            return Err("返回内容不是 LX 脚本".into());
        }
        write_cache(&filename, script.as_bytes())?;
        Ok(script)
    })();

    result.or_else(|error| {
        http.check()?;
        cached.ok_or(error)
    })
}

pub(super) fn prepare(
    http: &Http,
    track: &Track,
    sources: &[BuiltinSource],
    directory: &Path,
) -> Result<PreparedTrack, String> {
    let online = track.online.as_ref().ok_or("不是在线歌曲")?;
    let mut errors = Vec::new();
    let started = Instant::now();
    for source in sources {
        http.check()?;
        deadline(started, 240)?;

        let result: Result<PreparedTrack, String> = (|| {
            let raw = script_for(http, source, &directory.join("sources"))?;
            let mut engine = super::script::Source::load(http, &raw)?;
            let capability = &engine.sources[&online.source];
            if !capability["actions"]
                .as_array()
                .is_some_and(|actions| actions.iter().any(|action| action == "musicUrl"))
            {
                return Err("不支持该平台".into());
            }

            let mut qualities: Vec<_> = capability["qualitys"]
                .as_array()
                .ok_or("没有可用音质")?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();

            qualities.sort_by_key(|quality| match quality.as_str() {
                "320k" => 0,
                "192k" => 1,
                "128k" => 2,
                _ => 3,
            });
            qualities.dedup();
            if qualities.is_empty() {
                return Err("没有可用音质".into());
            }
            let mut quality_errors = Vec::new();
            for quality in qualities {
                http.check()?;
                deadline(started, 240)?;

                let attempt: Result<PreparedTrack, String> = (|| {
                    let url = engine.invoke(&json!({"source":online.source,"action":"musicUrl","info":{"type":quality,"musicInfo":online.music_info}}))?;
                    let url = url
                        .as_str()
                        .filter(|url| url.len() <= 8192)
                        .ok_or("播放地址无效")?;

                    let response =
                        http.fetch(url, &json!({"timeout":60000,"maxBytes":100*1024*1024}))?;

                    let mime = response.headers["content-type"]
                        .as_str()
                        .unwrap_or_default()
                        .to_ascii_lowercase();

                    if response.raw.len() < 1024
                        || ["text", "json", "html"]
                            .iter()
                            .any(|kind| mime.contains(kind))
                    {
                        return Err("播放地址未返回音频".into());
                    }

                    let file = directory.join("audio").join(format!(
                        "{}.audio",
                        key(&format!("{}{quality}", track.path.display()))
                    ));

                    write_cache(&file, &response.raw)?;
                    let (lyric, translated_lyric) = lyric(http, online).unwrap_or_default();
                    Ok(PreparedTrack {
                        file,
                        lyric,
                        translated_lyric,
                        source: source.name.into(),
                        quality: quality.clone(),
                    })
                })();

                match attempt {
                    Ok(track) => return Ok(track),
                    Err(error) => quality_errors.push(format!("{quality}：{error}")),
                }
            }
            Err(quality_errors.join("；"))
        })();

        match result {
            Ok(track) => return Ok(track),
            Err(error) => errors.push(format!("{}：{error}", source.name)),
        }
    }
    http.check()?;
    Err(format!("内置源均未能播放此歌曲。\n{}", errors.join("\n")))
}

fn lyric(http: &Http, online: &OnlineTrack) -> Result<(String, String), String> {
    if !matches!(online.source.as_str(), "wy" | "kw") {
        return Ok((String::new(), String::new()));
    }
    let id = text(&online.music_info["songmid"]);

    let mut url = reqwest::Url::parse(if online.source == "wy" {
        "https://music.163.com/api/song/lyric"
    } else {
        "https://m.kuwo.cn/newh5/singles/songinfoandlrc"
    })
    .map_err(|e| e.to_string())?;

    if online.source == "wy" {
        url.query_pairs_mut().extend_pairs([
            ("id", id.as_str()),
            ("lv", "-1"),
            ("kv", "-1"),
            ("tv", "-1"),
        ]);
    } else {
        url.query_pairs_mut().append_pair("musicId", &id);
    }

    let response = http.fetch(
        url.as_str(),
        &json!({"timeout":3000,"headers":{"Referer":"https://music.163.com/"}}),
    )?;

    if online.source == "wy" {
        return Ok((
            response.body["lrc"]["lyric"]
                .as_str()
                .unwrap_or_default()
                .into(),
            response.body["tlyric"]["lyric"]
                .as_str()
                .unwrap_or_default()
                .into(),
        ));
    }
    Ok((
        response.body["data"]["lrclist"]
            .as_array()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| {
                        let seconds = text(&line["time"]).parse::<f64>().unwrap_or(0.);
                        format!(
                            "[{}:{:05.2}]{}",
                            (seconds / 60.) as u64,
                            seconds % 60.,
                            line["lineLyric"].as_str().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        String::new(),
    ))
}
