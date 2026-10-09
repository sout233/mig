use crate::library::{LyricLine, Track, parse_lrc, parse_translated_lrc, storage_directory};
use lofty::{
    id3::v2::{Frame, Id3v2Tag, SyncTextContentType, SynchronizedTextFrame, TimestampFormat},
    picture::PictureType,
    prelude::{Accessor, AudioFile, TaggedFileExt},
    probe::Probe,
    tag::{ItemKey, TagType},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn enrich(track: &mut Track) -> Vec<LyricLine> {
    if track.online.is_some() {
        return Vec::new();
    }
    enrich_with_cache(track, &storage_directory().join("local-covers"))
}

fn enrich_with_cache(track: &mut Track, cache: &Path) -> Vec<LyricLine> {
    track.cover = track.cover.take().filter(|path| path.is_file());
    let mut embedded = Vec::new();
    if let Ok(tagged) = Probe::open(&track.path)
        .and_then(|probe| Ok(probe.guess_file_type()?))
        .and_then(|probe| probe.read())
    {
        let tags: Vec<_> = tagged
            .primary_tag()
            .into_iter()
            .chain(tagged.tags().iter())
            .collect();

        if let Some(title) = tags
            .iter()
            .find_map(|tag| tag.title())
            .filter(|text| !text.trim().is_empty())
        {
            track.title = title.into_owned();
        }
        if let Some(artist) = tags
            .iter()
            .find_map(|tag| tag.artist())
            .filter(|text| !text.trim().is_empty())
        {
            track.artist = artist.into_owned();
        }
        if let Some(album) = tags
            .iter()
            .find_map(|tag| tag.album())
            .filter(|text| !text.trim().is_empty())
        {
            track.album = album.into_owned();
        }
        let duration = tagged.properties().duration().as_secs_f64();
        if duration > 0. {
            track.duration = Some(duration);
        }
        let pictures = || tags.iter().flat_map(|tag| tag.pictures());
        track.cover = pictures()
            .filter(|picture| picture.pic_type() == PictureType::CoverFront)
            .chain(pictures())
            .find_map(|picture| cache_picture(picture.data(), cache))
            .or_else(|| track.cover.clone().filter(|path| path.is_file()));
        let mut plain = None;
        for tag in &tags {
            for text in [ItemKey::Lyrics, ItemKey::UnsyncLyrics]
                .into_iter()
                .flat_map(|key| tag.get_strings(key))
            {
                let lines = parse_lrc(text);
                if !lines.is_empty() {
                    embedded = lines;
                    break;
                }
                if plain.is_none() && !text.trim().is_empty() {
                    plain = Some(text.to_owned());
                }
            }
            if !embedded.is_empty() {
                break;
            }
            if tag.tag_type() == TagType::Id3v2 {
                let id3: Id3v2Tag = (*tag).clone().into();
                for frame in id3 {
                    if let Frame::Binary(binary) = frame
                        && binary.id().as_str() == "SYLT"
                        && let Ok(sync) = SynchronizedTextFrame::parse(&binary.data, binary.flags())
                        && sync.timestamp_format == TimestampFormat::MS
                        && sync.content_type == SyncTextContentType::Lyrics
                    {
                        embedded = sync
                            .content
                            .into_iter()
                            .map(|(time, text)| LyricLine {
                                seconds: f64::from(time) / 1000.,
                                text,
                                translation: None,
                            })
                            .collect();
                        embedded.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
                        break;
                    }
                }
            }
            if !embedded.is_empty() {
                break;
            }
        }
        if embedded.is_empty()
            && let Some(text) = plain
        {
            embedded = plain_lyrics(&text);
        }
    }
    if track.cover.is_none() {
        track.cover = external_cover(&track.path);
    }
    let original = fs::read_to_string(track.path.with_extension("lrc")).unwrap_or_default();

    let translated = ["t.lrc", "translated.lrc"]
        .into_iter()
        .filter_map(|ext| fs::read_to_string(track.path.with_extension(ext)).ok())
        .find(|text| !text.trim().is_empty())
        .unwrap_or_default();

    let external = parse_translated_lrc(&original, &translated);
    if !external.is_empty() {
        external
    } else if !original.trim().is_empty() {
        plain_lyrics(&original)
    } else {
        embedded
    }
}

fn plain_lyrics(text: &str) -> Vec<LyricLine> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| LyricLine {
            seconds: f64::INFINITY,
            text: line.to_owned(),
            translation: None,
        })
        .collect()
}

fn cache_picture(data: &[u8], directory: &Path) -> Option<PathBuf> {
    if data.len() > 16 * 1024 * 1024 {
        return None;
    }
    let path = directory.join(format!("{:x}.png", Sha256::digest(data)));
    if path.is_file() {
        return Some(path);
    }
    let image = image::load_from_memory(data).ok()?.thumbnail(1600, 1600);
    fs::create_dir_all(directory).ok()?;
    image
        .save_with_format(&path, image::ImageFormat::Png)
        .ok()?;
    Some(path)
}

fn external_cover(path: &Path) -> Option<PathBuf> {
    let directory = path.parent()?;
    ["jpg", "jpeg", "png", "webp"]
        .into_iter()
        .flat_map(|ext| {
            [
                path.with_extension(ext),
                directory.join(format!("cover.{ext}")),
                directory.join(format!("folder.{ext}")),
            ]
        })
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::{config::WriteOptions, picture::Picture, prelude::TagExt, tag::Tag};
    use std::io::Cursor;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut unique = [0; 8];
            getrandom::fill(&mut unique).unwrap();

            let path =
                std::env::temp_dir().join(format!("mig-tags-{:x}", u64::from_ne_bytes(unique)));

            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn audio(&self, lyrics: &str) -> PathBuf {
            let path = self.0.join("song.wav");
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&38u32.to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&8000u32.to_le_bytes());
            wav.extend_from_slice(&16000u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&2u32.to_le_bytes());
            wav.extend_from_slice(&[0, 0]);
            fs::write(&path, wav).unwrap();
            let mut tag = Tag::new(TagType::Id3v2);
            tag.set_title("内嵌标题".into());
            tag.set_artist("内嵌歌手".into());
            tag.set_album("内嵌专辑".into());
            tag.insert_text(ItemKey::UnsyncLyrics, lyrics.into());
            let mut png = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
                2,
                2,
                image::Rgba([30, 80, 100, 255]),
            ))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
            png.set_position(0);
            let mut picture = Picture::from_reader(&mut png).unwrap();
            picture.set_pic_type(PictureType::CoverFront);
            tag.push_picture(picture);
            tag.save_to_path(&path, WriteOptions::default()).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn embedded_artwork_and_lrc_are_read_from_real_audio_tags() {
        let fixture = Fixture::new();
        let path = fixture.audio("[00:01.00]第一句\n[00:02.00]第二句");
        let mut track = Track::from_path(path);
        let lines = enrich_with_cache(&mut track, &fixture.0.join("covers"));
        assert_eq!(track.title, "内嵌标题");
        assert_eq!(track.artist, "内嵌歌手");
        assert_eq!(track.album, "内嵌专辑");
        let cover = track.cover.unwrap();
        assert!(cover.is_file());
        assert!(image::open(cover).is_ok());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].seconds, 1.);
        assert_eq!(lines[0].text, "第一句");
    }

    #[test]
    fn plain_embedded_lyrics_are_visible_without_fabricated_timestamps() {
        let fixture = Fixture::new();
        let mut track = Track::from_path(fixture.audio("无时间歌词\n第二句"));
        let lines = enrich_with_cache(&mut track, &fixture.0.join("covers"));
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| !line.seconds.is_finite()));
    }

    #[test]
    fn external_lrc_and_translation_override_embedded_lyrics() {
        let fixture = Fixture::new();
        let path = fixture.audio("[00:01]内嵌歌词");
        fs::write(path.with_extension("lrc"), "[00:03]外置歌词").unwrap();
        fs::write(path.with_extension("t.lrc"), "[00:03]翻译").unwrap();
        let mut track = Track::from_path(path);
        let lines = enrich_with_cache(&mut track, &fixture.0.join("covers"));
        assert_eq!(lines[0].seconds, 3.);
        assert_eq!(lines[0].text, "外置歌词");
        assert_eq!(lines[0].translation.as_deref(), Some("翻译"));
    }
}
