use serde::{Deserialize, Serialize};
mod collections;
pub(crate) use collections::LocalCollection;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub(crate) struct Track {
    pub(crate) path: PathBuf,
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) album: String,
    pub(crate) duration: Option<f64>,
    pub(crate) favorite: bool,
    #[serde(default)]
    pub(crate) online: Option<crate::online::OnlineTrack>,
    #[serde(default)]
    pub(crate) cover: Option<PathBuf>,
}

impl Track {
    pub(crate) fn from_path(path: PathBuf) -> Self {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let (artist, title) = stem
            .split_once(" - ")
            .map(|(artist, title)| (artist.to_owned(), title.to_owned()))
            .unwrap_or_else(|| ("未知歌手".into(), stem.to_string()));

        let album = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        Self {
            path,
            title,
            artist,
            album,
            duration: None,
            favorite: false,
            online: None,
            cover: None,
        }
    }

    pub(crate) fn id(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }

    pub(crate) fn matches(&self, query: &str) -> bool {
        let haystack = format!("{} {} {}", self.title, self.artist, self.album).to_lowercase();
        query
            .split_whitespace()
            .all(|part| haystack.contains(&part.to_lowercase()))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedPlaylist {
    pub(crate) info: crate::online::PlaylistSummary,
    pub(crate) tracks: Vec<PathBuf>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub(crate) struct Library {
    pub(crate) tracks: Vec<Track>,
    pub(crate) recent: Vec<PathBuf>,
    pub(crate) playlists: Vec<SavedPlaylist>,
    pub(crate) collections: Vec<LocalCollection>,
    pub(crate) dark: bool,
    pub(crate) play_mode: crate::player::PlayMode,
    pub(crate) sidebar_width: Option<f32>,
    pub(crate) panel_width: Option<f32>,
    pub(crate) lyrics_spacing: Option<f32>,
    pub(crate) advanced_blur: Option<bool>,
}

impl Library {
    pub(crate) fn advanced_blur_enabled(&self) -> bool {
        cfg!(target_os = "windows") && self.advanced_blur.unwrap_or(true)
    }

    pub(crate) fn lyrics_spacing_percent(&self) -> f32 {
        self.lyrics_spacing
            .filter(|value| value.is_finite())
            .unwrap_or(100.)
            .clamp(80., 200.)
    }

    pub(crate) fn load() -> Result<Self, String> {
        let path = storage_directory().join("library.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path).map_err(|error| format!("无法读取音乐库：{error}"))?;
        Self::from_json(&text)
    }

    fn from_json(text: &str) -> Result<Self, String> {
        // Old libraries may still have the bundled demo tracks mixed in
        let mut data: serde_json::Value =
            serde_json::from_str(text).map_err(|error| format!("音乐库数据无法解析：{error}"))?;

        let mut removed = HashSet::new();
        if let Some(tracks) = data
            .get_mut("tracks")
            .and_then(serde_json::Value::as_array_mut)
        {
            tracks.retain(|track| {
                if track.get("demo").and_then(serde_json::Value::as_bool) == Some(true) {
                    if let Some(path) = track.get("path").and_then(serde_json::Value::as_str) {
                        removed.insert(PathBuf::from(path));
                    }
                    false
                } else {
                    true
                }
            });
        }

        let mut library: Self =
            serde_json::from_value(data).map_err(|error| format!("音乐库数据无法解析：{error}"))?;

        library.recent.retain(|path| !removed.contains(path));
        for playlist in &mut library.playlists {
            playlist.tracks.retain(|path| !removed.contains(path));
        }
        Ok(library)
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let directory = storage_directory();
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        let temporary = directory.join("library.json.tmp");
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        fs::rename(temporary, directory.join("library.json")).map_err(|error| error.to_string())
    }

    #[cfg(any(test, feature = "ui-verify"))]
    pub(crate) fn import(&mut self, paths: Vec<PathBuf>) -> usize {
        self.import_tracks(paths.into_iter().map(Track::from_path).collect())
    }

    pub(crate) fn import_tracks(&mut self, tracks: Vec<Track>) -> usize {
        let mut added = 0;
        for mut track in tracks {
            track.path = track.path.canonicalize().unwrap_or(track.path);
            if !is_audio(&track.path) {
                continue;
            }
            if let Some(existing) = self.tracks.iter_mut().find(|item| item.path == track.path) {
                track.favorite = existing.favorite;
                track.cover = track.cover.or_else(|| existing.cover.clone());
                *existing = track;
            } else {
                self.tracks.push(track);
                added += 1;
            }
        }
        added
    }

    pub(crate) fn record_play(&mut self, path: &Path) {
        self.recent.retain(|entry| entry != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(100);
    }
}

pub(crate) fn collect_audio_paths(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let mut pending = roots;
    let mut directories = HashSet::new();
    let mut files = HashSet::new();
    while let Some(path) = pending.pop() {
        let path = path
            .canonicalize()
            .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;

        if path.is_dir() {
            if !directories.insert(path.clone()) {
                continue;
            }

            let entries = fs::read_dir(&path)
                .map_err(|error| format!("无法读取文件夹 {}：{error}", path.display()))?;

            for entry in entries {
                let entry =
                    entry.map_err(|error| format!("无法读取文件夹 {}：{error}", path.display()))?;

                let kind = entry.file_type().map_err(|error| error.to_string())?;
                // Skip directory links so we do not wander outside this folder or loop forever
                if kind.is_dir() || (kind.is_file() && is_audio(&entry.path())) {
                    pending.push(entry.path());
                }
            }
        } else if path.is_file() && is_audio(&path) {
            files.insert(path);
        }
    }
    let mut files: Vec<_> = files.into_iter().collect();
    files.sort();
    Ok(files)
}

pub(crate) fn storage_directory() -> PathBuf {
    if let Some(directory) = std::env::var_os("MIG_MUSIC_DATA_DIR") {
        return directory.into();
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("MIG Music")
}

pub(crate) fn is_audio(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["mp3", "flac", "wav", "ogg"].contains(&extension.to_string_lossy().to_lowercase().as_str())
    })
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub(crate) struct LyricLine {
    pub(crate) seconds: f64,
    pub(crate) text: String,
    pub(crate) translation: Option<String>,
}

pub(crate) fn parse_lrc(text: &str) -> Vec<LyricLine> {
    let mut result = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim_start_matches('\u{feff}');
        let mut stamps = Vec::new();
        while let Some(tag) = rest.strip_prefix('[') {
            let Some((stamp, tail)) = tag.split_once(']') else {
                break;
            };

            if let Some((minutes, seconds)) = stamp.split_once(':')
                && let (Ok(minutes), Ok(seconds)) = (minutes.parse::<u32>(), seconds.parse::<f64>())
                && seconds.is_finite()
                && (0.0..60.0).contains(&seconds)
            {
                stamps.push(f64::from(minutes) * 60.0 + seconds);
            }
            rest = tail;
        }
        for seconds in stamps {
            result.push(LyricLine {
                seconds,
                text: rest.trim().to_owned(),
                translation: None,
            });
        }
    }
    result.sort_by(|left, right| left.seconds.total_cmp(&right.seconds));
    let mut grouped: Vec<LyricLine> = Vec::new();
    for line in result {
        if let Some(previous) = grouped.last_mut()
            && (previous.seconds - line.seconds).abs() < 0.001
        {
            if previous.text.is_empty() {
                previous.text = line.text;
            } else {
                append_translation(previous, &line.text);
            }
        } else {
            grouped.push(line);
        }
    }
    grouped
}

fn append_translation(line: &mut LyricLine, text: &str) {
    let text = text.trim();
    if text.is_empty()
        || text == line.text
        || line
            .translation
            .as_ref()
            .is_some_and(|existing| existing.lines().any(|part| part == text))
    {
        return;
    }
    match &mut line.translation {
        Some(existing) => {
            existing.push('\n');
            existing.push_str(text);
        }
        None => line.translation = Some(text.to_owned()),
    }
}

pub(crate) fn parse_translated_lrc(original: &str, translated: &str) -> Vec<LyricLine> {
    let mut lines = parse_lrc(original);
    let translations = parse_lrc(translated);
    if lines.is_empty() {
        return translations;
    }
    for translation in translations {
        let next = lines.partition_point(|line| line.seconds < translation.seconds);
        let ix = [next.checked_sub(1), (next < lines.len()).then_some(next)]
            .into_iter()
            .flatten()
            .min_by(|left, right| {
                (lines[*left].seconds - translation.seconds)
                    .abs()
                    .total_cmp(&(lines[*right].seconds - translation.seconds).abs())
            });
        // Original and translated LRC timestamps do not always match exactly
        if let Some(ix) = ix
            && (lines[ix].seconds - translation.seconds).abs() <= 0.15
        {
            append_translation(&mut lines[ix], &translation.text);
            if let Some(extra) = translation.translation {
                for text in extra.lines() {
                    append_translation(&mut lines[ix], text);
                }
            }
        }
    }
    lines
}

pub(crate) fn time_label(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_mode_persists_and_old_libraries_default_to_sequential() {
        use crate::player::PlayMode;
        assert_eq!(
            Library::from_json("{}").unwrap().play_mode,
            PlayMode::Sequential
        );
        for mode in [PlayMode::Sequential, PlayMode::Repeat, PlayMode::Shuffle] {
            let library = Library {
                play_mode: mode,
                ..Library::default()
            };
            let json = serde_json::to_string(&library).unwrap();
            assert_eq!(Library::from_json(&json).unwrap().play_mode, mode);
        }
        assert!(Library::from_json(r#"{"play_mode":"invalid"}"#).is_err());
    }

    #[test]
    fn advanced_blur_defaults_on_windows_and_persists_disabled() {
        let old = Library::from_json("{}").unwrap();
        assert_eq!(old.advanced_blur_enabled(), cfg!(target_os = "windows"));
        assert_eq!(
            Library::default().advanced_blur_enabled(),
            old.advanced_blur_enabled()
        );
        let mut library = old;
        library.advanced_blur = Some(false);
        let json = serde_json::to_string(&library).unwrap();
        assert!(!Library::from_json(&json).unwrap().advanced_blur_enabled());
    }
    #[test]
    fn older_library_discards_previews_but_preserves_imports_and_favorites() {
        let mut local = Track::from_path("用户歌曲.mp3".into());
        local.favorite = true;
        let mut preview = serde_json::to_value(Track::from_path("试听.wav".into())).unwrap();
        preview["demo"] = true.into();

        let text = serde_json::json!({
            "tracks": [preview, serde_json::to_value(&local).unwrap()],
            "recent": ["试听.wav", "用户歌曲.mp3"],
            "playlists": []
        })
        .to_string();

        let library = Library::from_json(&text).unwrap();
        assert_eq!(library.tracks.len(), 1);
        assert_eq!(library.tracks[0].path, local.path);
        assert!(library.tracks[0].favorite);
        assert_eq!(library.recent, vec![local.path]);
        assert!(!serde_json::to_string(&library).unwrap().contains("demo"));
    }

    #[test]
    fn folder_import_finds_nested_audio_and_deduplicates_overlapping_selections() {
        let root = std::env::temp_dir().join(format!(
            "mig-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        fs::create_dir_all(root.join("子文件夹")).unwrap();
        let first = root.join("单首.MP3");
        let second = root.join("子文件夹/第二首.flac");
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();
        fs::write(root.join("歌词.lrc"), []).unwrap();

        let paths =
            collect_audio_paths(vec![root.clone(), first.clone(), root.join("子文件夹")]).unwrap();

        let mut expected = vec![
            first.canonicalize().unwrap(),
            second.canonicalize().unwrap(),
        ];

        expected.sort();
        assert_eq!(paths, expected);
        let mut library = Library::default();
        assert_eq!(library.import(paths.clone()), 2);
        assert_eq!(library.import(paths), 0);
        assert!(collect_audio_paths(vec![root.join("不存在")]).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn translated_lyrics_align_by_time_without_creating_extra_playback_rows() {
        let lines = parse_translated_lrc(
            "[00:01.000]Hello\n[00:02.000]World\n[00:03.000]Again",
            "[00:00.990]你好\n[00:02.010]世界\n[00:10.000]不匹配",
        );

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(lines[1].translation.as_deref(), Some("世界"));
        assert_eq!(lines[2].translation, None);
        assert_eq!(lines[0].seconds, 1.);
    }

    #[test]
    fn bilingual_lrc_groups_same_timestamps_and_ignores_duplicate_text() {
        let lines = parse_translated_lrc(
            "[00:01.00]Hello\n[00:01.00]你好\n[00:01.00]你好\n[00:02.00]World",
            "[00:01.00]你好\n[00:02.00]World",
        );

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Hello");
        assert_eq!(lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(lines[1].translation, None);
    }

    #[test]
    fn absent_translation_preserves_original_and_translation_only_can_display() {
        let original = "[00:01]Hello\n[00:02]World";
        assert_eq!(parse_translated_lrc(original, ""), parse_lrc(original));
        assert_eq!(parse_translated_lrc("", "[00:01]你好")[0].text, "你好");
    }

    #[test]
    fn repeated_lyric_timestamps_are_sorted_and_metadata_is_ignored() {
        let lines = parse_lrc("[ar:歌手]\n[00:12.50][00:02.00]同一句\n[00:01.25]开场\n[00:99]无效");
        assert_eq!(
            lines.iter().map(|line| line.seconds).collect::<Vec<_>>(),
            [1.25, 2.0, 12.5]
        );
        assert_eq!(lines[2].text, "同一句");
    }

    #[test]
    fn import_deduplicates_and_search_matches_all_words() {
        let mut library = Library::default();
        assert_eq!(
            library.import(vec![
                "A - 夜色.MP3".into(),
                "A - 夜色.MP3".into(),
                "x.txt".into()
            ]),
            1
        );
        assert!(library.tracks[0].matches("a 夜色"));
        assert!(!library.tracks[0].matches("a 晨光"));
    }
}
