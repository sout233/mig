use super::motion::MotionButton as Button;
use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{
    dialog::{DialogAction, DialogClose, DialogFooter},
    input::Input,
    notification::Notification,
};

impl MigApp {
    pub(super) fn collection_dialog(
        &mut self,
        id: Option<u64>,
        track: Option<Track>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = id
            .and_then(|id| self.library.collections.iter().find(|item| item.id == id))
            .map(|item| item.name.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("收藏夹名称")
                .default_value(initial)
        });
        let entity = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let input = input.clone();
            let entity = entity.clone();
            let track = track.clone();
            dialog
                .title(if id.is_some() {
                    "重命名收藏夹"
                } else {
                    "新建收藏夹"
                })
                .child(
                    div()
                        .id("collection-name")
                        .test_support()
                        .child(Input::new(&input).bg(crate::theme::control_surface(cx))),
                )
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(|button| button.label("取消")))
                        .child(
                            DialogAction::new().child(
                                Button::new("save-local-collection").primary().label("保存"),
                            ),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    let name = input.read(cx).value().to_string();
                    entity
                        .update(cx, |this, cx| {
                            let result =
                                match id {
                                    Some(id) => this.library.rename_collection(id, &name),
                                    None => this.library.create_collection(&name).and_then(|id| {
                                        match track.clone() {
                                            Some(track) => {
                                                this.library.add_to_collection(id, track)
                                            }
                                            None => Ok(()),
                                        }
                                    }),
                                };
                            match result {
                                Ok(()) => {
                                    this.save();
                                    this.refresh_filter();
                                    cx.notify();
                                    true
                                }
                                Err(error) => {
                                    window.push_notification(Notification::error(error), cx);
                                    false
                                }
                            }
                        })
                        .expect("collection dialog owner dropped")
                })
        });
    }

    pub(super) fn choose_collection(
        &mut self,
        track: Track,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let collections = self.library.collections.clone();
        let entity = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let buttons = collections
                .iter()
                .map(|item| {
                    let id = item.id;
                    let track = track.clone();
                    let entity = entity.clone();
                    let added = item.tracks.contains(&track.path);
                    Button::new(SharedString::from(format!("choose-collection-{id}")))
                        .ghost()
                        .w_full()
                        .justify_start()
                        .label(item.name.clone())
                        .icon(if added {
                            Glyph::Check
                        } else {
                            Glyph::FolderPlus
                        })
                        .disabled(added)
                        .on_click(move |_, window, cx| {
                            entity
                                .update(cx, |this, cx| {
                                    match this.library.add_to_collection(id, track.clone()) {
                                        Ok(()) => {
                                            this.save();
                                            this.refresh_filter();
                                            cx.notify();
                                            window.close_dialog(cx);
                                            window.push_notification(
                                                Notification::success("已添加到收藏夹"),
                                                cx,
                                            );
                                        }
                                        Err(error) => {
                                            window.push_notification(Notification::error(error), cx)
                                        }
                                    }
                                })
                                .expect("collection picker owner dropped");
                        })
                })
                .collect::<Vec<_>>();
            let entity = entity.clone();
            let track = track.clone();
            dialog
                .title("添加到收藏夹")
                .child(
                    v_flex()
                        .id("collection-picker")
                        .max_h(rems(20.))
                        .overflow_y_scroll()
                        .children(buttons),
                )
                .child(
                    Button::new("create-collection-for-track")
                        .label("新建收藏夹…")
                        .icon(Glyph::Plus)
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            entity
                                .update(cx, |this, cx| {
                                    this.collection_dialog(None, Some(track.clone()), window, cx)
                                })
                                .expect("collection picker owner dropped");
                        }),
                )
        });
    }

    pub(super) fn collection_controls(&self, cx: &mut Context<Self>) -> Option<Div> {
        let Page::Collection(id) = self.page else {
            return None;
        };
        let name = self
            .library
            .collections
            .iter()
            .find(|item| item.id == id)?
            .name
            .clone();
        Some(
            h_flex()
                .gap_2()
                .child(
                    Button::new("rename-collection")
                        .ghost()
                        .icon(Glyph::Pencil)
                        .tooltip("重命名收藏夹")
                        .accessibility_label("重命名收藏夹")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.collection_dialog(Some(id), None, window, cx)
                        })),
                )
                .child(
                    Button::new("delete-collection")
                        .ghost()
                        .icon(Glyph::Trash)
                        .tooltip("删除收藏夹")
                        .accessibility_label("删除收藏夹")
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let entity = cx.entity().downgrade();
                            let name = name.clone();
                            window.open_dialog(cx, move |dialog, _, _| {
                                let entity = entity.clone();
                                dialog
                                    .title(format!("删除收藏夹“{name}”？"))
                                    .child("歌曲文件和“我喜欢的”收藏仍会保留。")
                                    .footer(
                                        DialogFooter::new()
                                            .child(
                                                DialogClose::new()
                                                    .trigger(|button| button.label("取消")),
                                            )
                                            .child(
                                                DialogAction::new().child(
                                                    Button::new("confirm-delete-collection")
                                                        .label("删除"),
                                                ),
                                            ),
                                    )
                                    .on_ok(move |_, window, cx| {
                                        entity
                                            .update(cx, |this, cx| {
                                                this.library
                                                    .collections
                                                    .retain(|item| item.id != id);
                                                this.save();
                                                this.navigate(Page::Favorites, window, cx);
                                                this.navigation_back.retain(|entry| {
                                                    entry.page != Page::Collection(id)
                                                });
                                                this.navigation_forward.retain(|entry| {
                                                    entry.page != Page::Collection(id)
                                                });
                                            })
                                            .expect("collection deletion owner dropped");
                                        true
                                    })
                            });
                        })),
                ),
        )
    }

    pub(super) fn local_collection_links(&self, cx: &mut Context<Self>) -> Vec<Button> {
        self.library
            .collections
            .iter()
            .map(|item| {
                let id = item.id;
                Button::new(SharedString::from(format!("local-collection-{id}")))
                    .ghost()
                    .w_full()
                    .h(rems(3.))
                    .flex_shrink_0()
                    .justify_start()
                    .selected(self.page == Page::Collection(id))
                    .icon(Glyph::Folder)
                    .child(div().flex_1().min_w_0().truncate().child(item.name.clone()))
                    .tooltip(item.name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate(Page::Collection(id), window, cx)
                    }))
            })
            .collect()
    }

    pub(super) fn remove_collection_track(
        &mut self,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        if let Page::Collection(id) = self.page {
            let item = self
                .library
                .collections
                .iter_mut()
                .find(|item| item.id == id)
                .expect("active collection exists");
            item.tracks.retain(|item| item != path);
            self.save();
            self.refresh_filter();
            cx.notify();
        }
    }
}
