use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::base::TestSupportExt;

pub(super) enum HighResolutionCover {
    Pending,
    Downloaded(PathBuf),
    Ready(PathBuf),
    Failed,
}

impl MigApp {
    pub(super) fn load_high_resolution_cover(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.expanded_lyrics {
            return;
        }
        let Some(track) = self.current.as_ref().filter(|track| {
            track
                .online
                .as_ref()
                .is_some_and(|online| online.source == "wy")
        }) else {
            return;
        };
        let Some(thumbnail) = track.cover.clone() else {
            return;
        };
        let downloaded = match self.high_resolution_covers.get(&thumbnail) {
            Some(HighResolutionCover::Downloaded(path)) => Some(path.clone()),
            Some(_) => return,
            None => None,
        };
        let low_image = match window.get_asset::<ImgResourceLoader>(&thumbnail.clone().into(), cx) {
            Some(Ok(image)) => image,
            Some(Err(error)) => {
                eprintln!("低清封面解码失败，无法升级高清封面：{error}");
                self.high_resolution_covers
                    .insert(thumbnail, HighResolutionCover::Failed);
                return;
            }
            None => return,
        };
        if let Some(path) = downloaded {
            match window.get_asset::<ImgResourceLoader>(&path.clone().into(), cx) {
                Some(Ok(image)) => {
                    let low_size = low_image.size(0);
                    let high_size = image.size(0);
                    if high_size.width > low_size.width && high_size.height > low_size.height {
                        self.high_resolution_covers
                            .insert(thumbnail, HighResolutionCover::Ready(path));
                        cx.notify();
                    } else {
                        eprintln!("高清封面分辨率未提升：{low_size:?} → {high_size:?}");
                        self.high_resolution_covers
                            .insert(thumbnail, HighResolutionCover::Failed);
                    }
                }
                Some(Err(error)) => {
                    eprintln!("高清封面解码失败：{error}");
                    self.high_resolution_covers
                        .insert(thumbnail, HighResolutionCover::Failed);
                }
                None => {}
            }
            return;
        }
        let track = track.clone();
        self.high_resolution_covers
            .insert(thumbnail.clone(), HighResolutionCover::Pending);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { crate::online::high_resolution_cover(&track) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let state = match result {
                    Ok(path) => HighResolutionCover::Downloaded(path),
                    Err(error) => {
                        eprintln!("高清封面获取失败（{}）：{error}", thumbnail.display());
                        HighResolutionCover::Failed
                    }
                };
                this.high_resolution_covers.insert(thumbnail, state);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn expanded_cover_path(&self) -> Option<PathBuf> {
        let thumbnail = self.current.as_ref()?.cover.as_ref()?;
        match self.high_resolution_covers.get(thumbnail) {
            Some(HighResolutionCover::Ready(path)) => Some(path.clone()),
            _ => Some(thumbnail.clone()),
        }
    }

    pub(super) fn refresh_cover_theme(&mut self, cx: &mut Context<Self>) {
        let cover = self.current.as_ref().and_then(|track| track.cover.clone());
        if self.theme_cover == cover {
            return;
        }
        self.theme_cover = cover.clone();

        let Some(path) = cover else {
            self.theme_accent = None;
            crate::theme::apply_cover(self.library.dark, None, cx);
            return;
        };

        if let Some(color) = self.cover_colors.get(&path).copied() {
            self.theme_accent = color;
            crate::theme::apply_cover(self.library.dark, color, cx);
            return;
        }
        cx.spawn(async move |this, cx| {
            let image = path.clone();
            let color = cx
                .background_spawn(async move { crate::theme::extract_cover(&image) })
                .await;

            let _ = this.update(cx, |this, cx| {
                if this.cover_colors.len() >= 128 {
                    this.cover_colors.clear();
                }
                this.cover_colors.insert(path.clone(), color);
                // If we already switched tracks, ignore this old cover result
                if this.theme_cover.as_ref() == Some(&path) {
                    this.theme_accent = color;
                    crate::theme::apply_cover(this.library.dark, color, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn load_covers(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.cover_busy {
            return;
        }

        let top = {
            let state = self.scroll.0.borrow();
            state
                .last_item_size
                .map(|size| {
                    if size.item.height > px(0.) {
                        (-state.base_handle.offset().y / size.item.height).max(0.) as usize
                    } else {
                        0
                    }
                })
                .unwrap_or(0)
        };

        let mut candidates = Vec::new();
        if let Some(current) = &self.current {
            candidates.push(current.clone());
        }
        if !matches!(self.page, Page::Settings | Page::Sources) {
            candidates.extend(
                self.visible
                    .iter()
                    .skip(top)
                    .take(20)
                    .map(|ix| self.library.tracks[*ix].clone()),
            );
        }

        let tracks: Vec<_> = candidates
            .into_iter()
            .filter(|track| {
                track.online.is_some()
                    && track.cover.is_none()
                    && self.cover_requested.insert(track.path.clone())
            })
            .collect();

        if tracks.is_empty() {
            return;
        }
        self.cover_busy = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { crate::online::covers(&tracks) })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.cover_busy = false;
                if let Ok(covers) = result {
                    let mut changed = false;
                    for cover in covers {
                        if let Some(track) = this
                            .library
                            .tracks
                            .iter_mut()
                            .find(|track| track.path == cover.id)
                        {
                            if let Some(url) = &cover.url
                                && let Some(online) = &mut track.online
                            {
                                online.music_info["img"] = serde_json::json!(url);
                            }
                            track.cover = cover.file.clone();
                            changed |= cover.file.is_some();
                        }
                        if let Some(current) = &mut this.current
                            && current.path == cover.id
                        {
                            current.cover = cover.file;
                        }
                    }
                    if changed {
                        this.save();
                    }
                }
                this.refresh_cover_theme(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn album_cover(
        &self,
        track: Option<&Track>,
        player: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let size = if player { rems(3.875) } else { rems(2.5) };
        let icon_size = if player { rems(1.75) } else { rems(1.25) };
        let glyph = if player { Glyph::Disc3 } else { Glyph::Music2 };
        let foreground = cx.theme().primary;
        let fallback = move || {
            h_flex()
                .size_full()
                .justify_center()
                .text_color(foreground)
                .child(Icon::new(glyph).size(icon_size))
                .into_any_element()
        };

        let id = if player {
            "player-cover".to_owned()
        } else {
            format!("cover-{}", track.map(Track::id).unwrap_or_default())
        };

        div()
            .id(SharedString::from(id))
            .size(size)
            .flex_shrink_0()
            .rounded(cx.theme().radius)
            .overflow_hidden()
            .bg(cx.theme().muted)
            .child(
                if let Some(path) = track.and_then(|track| track.cover.clone()) {
                    img(path)
                        .size_full()
                        .rounded(cx.theme().radius)
                        .object_fit(ObjectFit::Cover)
                        .with_loading(fallback)
                        .with_fallback(fallback)
                        .into_any_element()
                } else {
                    fallback()
                },
            )
            .test_support()
            .into_any_element()
    }
}
