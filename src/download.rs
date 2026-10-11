use crate::library::Track;
use lofty::{
    config::WriteOptions,
    file::FileType,
    picture::{Picture, PictureType},
    prelude::{Accessor, AudioFile, TaggedFileExt},
    probe::Probe,
    tag::{ItemKey, Tag, TagType},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

pub(crate) struct Download {
    pub(crate) source: PathBuf,
    pub(crate) extension: &'static str,
    pub(crate) lyric: String,
    pub(crate) translation: String,
}

pub(crate) fn prepare(track: &Track) -> Result<Download, String> {
    let prepared = crate::online::prepare(track, Arc::<AtomicBool>::default())?;
    let probe = Probe::open(&prepared.file)
        .and_then(|probe| Ok(probe.guess_file_type()?))
        .map_err(|error| format!("无法识别下载音频：{error}"))?;
    let extension = match probe.file_type().ok_or("下载内容不是支持的音频")? {
        FileType::Mpeg => "mp3",
        FileType::Flac => "flac",
        FileType::Vorbis | FileType::Opus => "ogg",
        FileType::Wav => "wav",
        FileType::Mp4 => "m4a",
        kind => return Err(format!("暂不支持导出此音频格式：{kind:?}")),
    };
    Ok(Download {
        source: prepared.file,
        extension,
        lyric: prepared.lyric,
        translation: prepared.translated_lyric,
    })
}

pub(crate) fn filename(track: &Track) -> String {
    let name = format!("{} - {}", track.artist, track.title);
    let name: String = name
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    name.trim_matches([' ', '.']).to_owned()
}

pub(crate) fn save(download: Download, track: &Track, path: &Path) -> Result<(), String> {
    let mut file = Probe::open(&download.source)
        .and_then(|probe| Ok(probe.guess_file_type()?))
        .and_then(|probe| probe.read())
        .map_err(|error| format!("无法读取下载音频：{error}"))?;
    let tag_type = if file.file_type() == FileType::Wav {
        TagType::Id3v2
    } else {
        file.primary_tag_type()
    };
    if file.tag(tag_type).is_none() {
        file.insert_tag(Tag::new(tag_type));
    }
    let tag = file.tag_mut(tag_type).expect("audio tag inserted");
    tag.set_title(track.title.clone());
    tag.set_artist(track.artist.clone());
    tag.set_album(track.album.clone());
    if !download.lyric.is_empty() {
        tag.insert_text(ItemKey::Lyrics, download.lyric.clone());
    }
    if let Some(cover) = &track.cover {
        let bytes = fs::read(cover).map_err(|error| format!("无法读取封面：{error}"))?;
        let mut picture = Picture::from_reader(&mut std::io::Cursor::new(bytes))
            .map_err(|error| format!("无法读取封面图片：{error}"))?;
        picture.set_pic_type(PictureType::CoverFront);
        tag.push_picture(picture);
    }
    fs::copy(&download.source, path).map_err(|error| format!("无法保存音频：{error}"))?;
    file.save_to_path(path, WriteOptions::default())
        .map_err(|error| format!("无法写入歌曲信息：{error}"))?;
    if !download.lyric.is_empty() {
        fs::write(path.with_extension("lrc"), &download.lyric)
            .map_err(|error| format!("无法保存歌词：{error}"))?;
    }
    if !download.translation.is_empty() {
        fs::write(path.with_extension("translated.lrc"), &download.translation)
            .map_err(|error| format!("无法保存翻译歌词：{error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_name_cannot_escape_destination_directory() {
        let mut track = Track::from_path("song.mp3".into());
        track.title = "../song: \"one\"?".into();
        track.artist = "artist\\two".into();
        let name = filename(&track);
        assert!(!name.contains(['/', '\\', ':', '"', '?']));
        assert_eq!(Path::new(&name).components().count(), 1);
    }

    #[test]
    fn exported_audio_keeps_metadata_and_bilingual_lyrics() {
        let root = std::env::temp_dir().join(format!(
            "mig-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.audio");
        let destination = root.join("song.wav");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&356u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&320u32.to_le_bytes());
        bytes.resize(364, 0);
        fs::write(&source, bytes).unwrap();
        let mut track = Track::from_path(source.clone());
        track.title = "测试标题".into();
        track.artist = "测试歌手".into();
        track.album = "测试专辑".into();
        let cover = root.join("cover.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([64, 128, 192]))
            .save(&cover)
            .unwrap();
        track.cover = Some(cover);
        save(
            Download {
                source,
                extension: "wav",
                lyric: "[00:00.00]Hello".into(),
                translation: "[00:00.00]你好".into(),
            },
            &track,
            &destination,
        )
        .unwrap();
        let file = Probe::open(&destination).unwrap().read().unwrap();
        assert_eq!(
            file.tag(TagType::Id3v2).unwrap().title().as_deref(),
            Some("测试标题")
        );
        assert_eq!(
            file.tag(TagType::Id3v2).unwrap().artist().as_deref(),
            Some("测试歌手")
        );
        let mut exported = Track::from_path(destination);
        let lyrics = crate::local_metadata::enrich(&mut exported);
        assert!(exported.cover.is_some());
        assert_eq!(lyrics[0].text, "Hello");
        assert_eq!(lyrics[0].translation.as_deref(), Some("你好"));
        fs::remove_dir_all(root).unwrap();
    }
}
