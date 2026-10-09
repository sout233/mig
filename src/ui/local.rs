use super::motion::MotionButton as Button;
use super::*;
use gpui_kit::assets::IconName as Glyph;

impl MigApp {
    pub(super) fn local_import_buttons(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .child(
                Button::new("local-import-files")
                    .label("导入歌曲…")
                    .icon(Glyph::FilePlus)
                    .disabled(self.loading || self.importing)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.import_local(false, window, cx)),
                    ),
            )
            .child(
                Button::new("local-import-folder")
                    .label("导入文件夹…")
                    .icon(Glyph::FolderPlus)
                    .disabled(self.loading || self.importing)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.import_local(true, window, cx)),
                    ),
            )
    }

    pub(super) fn import_local(
        &mut self,
        folder: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.importing {
            return;
        }
        self.navigate(Page::Local, window, cx);
        self.importing = true;
        self.local_import_status = Some("正在选择文件…".into());
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let dialog = rfd::AsyncFileDialog::new();

            let selected = if folder {
                dialog
                    .set_title("导入音乐文件夹")
                    .pick_folder()
                    .await
                    .map(|file| vec![file.path().to_owned()])
            } else {
                dialog
                    .set_title("导入歌曲（可多选）")
                    .add_filter("音频文件", &["mp3", "flac", "wav", "ogg"])
                    .pick_files()
                    .await
                    .map(|files| {
                        files
                            .into_iter()
                            .map(|file| file.path().to_owned())
                            .collect()
                    })
            };

            let result = if let Some(paths) = selected {
                let _ = this.update_in(cx, |this, _, cx| {
                    this.local_import_status = Some("正在读取本地音乐…".into());
                    cx.notify();
                });

                Some(
                    cx.background_spawn(async move {
                        let paths = crate::library::collect_audio_paths(paths)?;
                        Ok::<_, String>(
                            paths
                                .into_iter()
                                .map(|path| {
                                    let mut track = Track::from_path(path);
                                    crate::local_metadata::enrich(&mut track);
                                    track
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .await,
                )
            } else {
                None
            };

            let _ = this.update_in(cx, |this, _, cx| {
                match result {
                    Some(Ok(paths)) => {
                        let found = paths.len();
                        let added = this.library.import_tracks(paths);
                        this.local_import_status = Some(if found == 0 {
                            "所选位置没有支持的音频文件".into()
                        } else if added == 0 {
                            "所选歌曲已导入，没有新增歌曲".into()
                        } else {
                            format!("已导入 {added} 首歌曲")
                        });
                        this.refresh_filter();
                        if found > 0 {
                            this.save();
                        }
                    }
                    Some(Err(error)) => {
                        this.local_import_status = Some("导入失败，请重试".into());
                        this.error = Some(error);
                    }
                    None => this.local_import_status = None,
                }
                this.importing = false;
                cx.notify();
            });
        })
        .detach();
    }
}
