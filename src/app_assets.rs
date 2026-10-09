use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;

const ICONS: [(&str, &[u8]); 5] = [
    (
        "icons/mig/heart-solid.svg",
        include_bytes!("../assets/icons/heart-solid.svg"),
    ),
    (
        "icons/mig/play-solid.svg",
        include_bytes!("../assets/icons/play-solid.svg"),
    ),
    (
        "icons/mig/pause-solid.svg",
        include_bytes!("../assets/icons/pause-solid.svg"),
    ),
    (
        "icons/mig/previous-solid.svg",
        include_bytes!("../assets/icons/previous-solid.svg"),
    ),
    (
        "icons/mig/next-solid.svg",
        include_bytes!("../assets/icons/next-solid.svg"),
    ),
];

pub(crate) struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::AllAssets.list(path)?;
        paths.extend(
            ICONS
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
