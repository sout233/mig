use super::{Library, Track};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LocalCollection {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) tracks: Vec<PathBuf>,
}

impl Library {
    pub(crate) fn create_collection(&mut self, name: &str) -> Result<u64, String> {
        let name = self.collection_name(name, None)?;
        let id = self
            .collections
            .iter()
            .map(|item| item.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("收藏夹 ID 已耗尽")?;
        self.collections.push(LocalCollection {
            id,
            name,
            tracks: Vec::new(),
        });
        Ok(id)
    }

    pub(crate) fn rename_collection(&mut self, id: u64, name: &str) -> Result<(), String> {
        let name = self.collection_name(name, Some(id))?;
        let collection = self
            .collections
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("收藏夹不存在")?;
        collection.name = name;
        Ok(())
    }

    fn collection_name(&self, name: &str, except: Option<u64>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("请输入收藏夹名称".into());
        }
        if self
            .collections
            .iter()
            .any(|item| Some(item.id) != except && item.name == name)
        {
            return Err("已有同名收藏夹".into());
        }
        Ok(name.to_owned())
    }

    pub(crate) fn add_to_collection(&mut self, id: u64, track: Track) -> Result<(), String> {
        let collection = self
            .collections
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("收藏夹不存在")?;
        if !collection.tracks.contains(&track.path) {
            collection.tracks.push(track.path.clone());
        }
        if !self.tracks.iter().any(|item| item.path == track.path) {
            self.tracks.push(track);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collections_survive_serialization_and_do_not_duplicate_tracks() {
        let mut library = Library::default();
        let id = library.create_collection(" 夜晚 ").unwrap();
        let track = Track::from_path("song.mp3".into());
        library.add_to_collection(id, track.clone()).unwrap();
        library.add_to_collection(id, track).unwrap();
        library.rename_collection(id, "日常").unwrap();
        let library: Library =
            serde_json::from_str(&serde_json::to_string(&library).unwrap()).unwrap();
        assert_eq!(library.collections[0].name, "日常");
        assert_eq!(library.collections[0].tracks.len(), 1);
        assert_eq!(library.tracks.len(), 1);
    }

    #[test]
    fn invalid_names_and_missing_collections_are_reported() {
        let mut library = Library::default();
        assert!(library.create_collection(" ").is_err());
        let id = library.create_collection("测试").unwrap();
        assert!(library.create_collection("测试").is_err());
        assert!(library.rename_collection(id + 1, "另一个").is_err());
        assert!(
            library
                .add_to_collection(id + 1, Track::from_path("a.mp3".into()))
                .is_err()
        );
        let old: Library = serde_json::from_str("{}").unwrap();
        assert!(old.collections.is_empty());
    }
}
