use super::*;
use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::tooltip::Tooltip;

impl MigApp {
    pub(super) fn favorite_icon(favorite: bool) -> Icon {
        if favorite {
            Icon::default()
                .path("icons/mig/heart-solid.svg")
                .text_color(crate::theme::favorite_color())
        } else {
            Icon::new(Glyph::Heart)
        }
    }

    pub(super) fn player_title(&self, track: Option<&Track>) -> impl IntoElement {
        let title: SharedString = track
            .map(|track| track.title.replace(['\n', '\r'], " "))
            .unwrap_or_else(|| "滚木".into())
            .into();

        let tooltip = title.clone();
        let started = self.title_started;
        div()
            .id("player-title")
            .w_full()
            .min_w_0()
            .h(rems(1.5))
            .overflow_hidden()
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .child(
                canvas(
                    move |_, window, _| {
                        let mut style = window.text_style();
                        style.font_weight = FontWeight::BOLD;
                        // Keep the old title scale, just make it about 2px bigger
                        let font_size = window.rem_size() * 0.875 + px(2.);
                        let run = style.to_run(title.len());
                        window
                            .text_system()
                            .shape_line(title, font_size, &[run], None)
                    },
                    move |bounds, line, window, cx| {
                        let overflow = (line.width - bounds.size.width).max(px(0.));
                        let offset = if overflow > px(0.) && !cx.reduce_motion() {
                            // Pause at each end for two seconds, then scroll at 24px per second
                            let travel = f32::from(overflow) / 24.;
                            let elapsed = started.elapsed().as_secs_f32() % (travel * 2. + 4.);
                            window.request_animation_frame();
                            if elapsed < 2. {
                                px(0.)
                            } else if elapsed < 2. + travel {
                                px((elapsed - 2.) * 24.)
                            } else if elapsed < 4. + travel {
                                overflow
                            } else {
                                px(((travel * 2. + 4.) - elapsed) * 24.)
                            }
                        } else {
                            px(0.)
                        };

                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            let _ = line.paint(
                                bounds.origin - point(offset, px(0.)),
                                bounds.size.height,
                                TextAlign::Left,
                                None,
                                window,
                                cx,
                            );
                        });
                    },
                )
                .size_full(),
            )
    }
}
