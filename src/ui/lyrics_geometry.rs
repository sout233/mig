use super::*;
use std::ops::Range;

#[derive(PartialEq)]
struct GeometryKey {
    width: Pixels,
    rem: Pixels,
    font: Font,
    spacing: f32,
    source: usize,
    count: usize,
}

#[derive(Default)]
pub(super) struct LyricsGeometry {
    key: Option<GeometryKey>,
    offsets: Vec<Pixels>,
}

impl LyricsGeometry {
    pub(super) fn update(
        &mut self,
        lines: &[LyricLine],
        width: Pixels,
        spacing: f32,
        window: &mut Window,
    ) -> bool {
        let style = window.text_style();
        let key = GeometryKey {
            width,
            rem: window.rem_size(),
            font: style.to_run(0).font,
            spacing,
            source: lines.as_ptr() as usize,
            count: lines.len(),
        };
        if self.key.as_ref() == Some(&key) {
            return false;
        }

        let mut offsets = Vec::with_capacity(lines.len() + 1);
        offsets.push(px(0.));
        for line in lines {
            let original: SharedString = if line.text.trim().is_empty() {
                "♪".into()
            } else {
                line.text.clone().into()
            };
            let mut text_style = style.clone();
            text_style.font_weight = FontWeight::BOLD;
            let rem = window.rem_size();
            let original_height =
                measured_height(original, rem * 2., rem * 2.4, width, &text_style, window);
            let translation_height = line.translation.as_ref().map(|text| {
                text_style.font_weight = FontWeight::NORMAL;
                measured_height(
                    text.clone().into(),
                    rem * 1.125,
                    rem * 1.575,
                    width,
                    &text_style,
                    window,
                )
            });
            let height = row_height(rem, spacing, original_height, translation_height);
            offsets.push(*offsets.last().unwrap() + height);
        }
        self.offsets = offsets;
        self.key = Some(key);
        true
    }

    pub(super) fn height(&self, index: usize) -> Pixels {
        self.offsets[index + 1] - self.offsets[index]
    }

    pub(super) fn top(&self, index: usize) -> Pixels {
        self.offsets[index]
    }

    pub(super) fn total(&self) -> Pixels {
        self.offsets.last().copied().unwrap_or(px(0.))
    }

    pub(super) fn width(&self) -> Pixels {
        self.key.as_ref().map_or(px(0.), |key| key.width)
    }

    pub(super) fn visible(&self, top: Pixels, viewport: Pixels) -> Range<usize> {
        let count = self.offsets.len().saturating_sub(1);
        let start = self
            .offsets
            .partition_point(|offset| *offset <= top)
            .saturating_sub(1);
        let end = self
            .offsets
            .partition_point(|offset| *offset < top + viewport);
        start.saturating_sub(2).min(count)..(end + 2).min(count)
    }
}

fn measured_height(
    text: SharedString,
    font_size: Pixels,
    line_height: Pixels,
    width: Pixels,
    style: &TextStyle,
    window: &mut Window,
) -> Pixels {
    window
        .text_system()
        .shape_text(
            text.clone(),
            font_size,
            &[style.to_run(text.len())],
            Some(width),
            None,
        )
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.size(line_height).height)
                .fold(px(0.), |sum, height| sum + height)
        })
        .unwrap_or(line_height)
}

fn row_height(rem: Pixels, spacing: f32, original: Pixels, translation: Option<Pixels>) -> Pixels {
    let base = if translation.is_some() { 7.2 } else { 4.8 };
    let extra = (original - rem * 2.4).max(px(0.))
        + translation.map_or(px(0.), |height| (height - rem * 1.575).max(px(0.)));
    rem * (base + (spacing - 100.) * 0.015) + extra
}

#[cfg(test)]
mod tests {
    use super::{LyricsGeometry, row_height};
    use core::prelude::v1::test;
    use gpui_kit::px;

    #[test]
    fn wrapped_original_and_translation_keep_the_same_surrounding_space() {
        let rem = px(16.);
        let short = row_height(rem, 100., rem * 2.4, Some(rem * 1.575));
        let long = row_height(rem, 100., rem * 7.2, Some(rem * 3.15));
        assert!((short - rem * 7.2).abs() < px(0.001));
        assert!((long - short - rem * (4.8 + 1.575)).abs() < px(0.001));
        assert!(long > rem * (7.2 + 3.15 + 0.25));
        assert!((row_height(rem, 100., rem * 2.4, None) - rem * 4.8).abs() < px(0.001));
    }

    #[test]
    fn visible_range_includes_tall_rows_and_uses_cumulative_heights() {
        let geometry = LyricsGeometry {
            key: None,
            offsets: vec![
                px(0.),
                px(80.),
                px(400.),
                px(480.),
                px(560.),
                px(640.),
                px(720.),
            ],
        };
        assert_eq!(geometry.height(1), px(320.));
        assert_eq!(geometry.top(2), px(400.));
        assert!(geometry.visible(px(300.), px(50.)).contains(&1));
        assert_eq!(geometry.total(), px(720.));
    }
}
