use super::{http::Http, playlist, service};
use crate::library::{Track, storage_directory};
use std::{path::PathBuf, sync::Arc};

pub(crate) fn high_resolution_cover(track: &Track) -> Result<PathBuf, String> {
    let online = track.online.as_ref().ok_or("不是在线歌曲")?;
    if online.source != "wy" {
        return Err("高清封面接口仅支持网易云歌曲".into());
    }
    let id = online.music_info["songmid"]
        .as_u64()
        .or_else(|| online.music_info["songmid"].as_str()?.parse().ok())
        .filter(|id| *id > 0)
        .ok_or("歌曲 ID 无效")?;
    let http = Http::new(Arc::default());
    let body = playlist::get(
        &http,
        "https://ncm-api.soout.top",
        "/song/detail",
        &[("ids", id.to_string())],
    )?;
    let song = body["songs"]
        .as_array()
        .and_then(|songs| songs.iter().find(|song| song["id"].as_u64() == Some(id)))
        .ok_or("歌曲详情中缺少对应歌曲")?;
    let album = song.get("al").or_else(|| song.get("album"));
    let url = album
        .and_then(|album| album["picUrl"].as_str())
        .filter(|url| !url.is_empty())
        .ok_or("歌曲详情中缺少专辑封面")?;
    service::cache_cover_with_size(
        &http,
        url,
        &storage_directory().join("online/covers-hd"),
        1024,
    )
}
