use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    ContentMask, FontWeight, Hsla, IntoElement, Pixels, Point, SharedString, Styled, TextAlign,
    TextStyle, Window, WrappedLine, canvas, point, px,
};

use std::rc::Rc;

#[derive(PartialEq)]
struct LayoutKey {
    original: SharedString,
    translation: Option<SharedString>,
    width: Pixels,
    rem: Pixels,
    scale: f32,
    style: TextStyle,
    foreground: Hsla,
    muted_foreground: Hsla,
    passes: usize,
}

type TextLayout = Vec<(WrappedLine, Pixels, Pixels)>;

#[derive(Default)]
struct LayoutCache {
    key: Option<LayoutKey>,
    lines: Rc<TextLayout>,
}

/// Advanced blur samples the glyph texture on the GPU; the light mode uses offset copies
pub(super) fn lyric_text(
    id: SharedString,
    original: SharedString,
    translation: Option<SharedString>,
    blur: f32,
    scale: f32,
    advanced: bool,
) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| {
            let radius = if bounds.contains(&window.mouse_position()) {
                px(0.)
            } else {
                window.rem_size() * blur
            };

            let offsets = blur_offsets(if advanced { px(0.) } else { radius });
            let gpu_radius = if advanced { radius } else { px(0.) };
            let mut style = window.text_style();
            let key = LayoutKey {
                original: original.clone(),
                translation: translation.clone(),
                width: bounds.size.width,
                rem: window.rem_size(),
                scale,
                style: style.clone(),
                foreground: cx.theme().foreground,
                muted_foreground: cx.theme().muted_foreground,
                passes: offsets.len(),
            };
            let cache = window.use_keyed_state(id.clone(), cx, |_, _| LayoutCache::default());
            if cache.read(cx).key.as_ref() == Some(&key) {
                return (cache.read(cx).lines.clone(), offsets, gpu_radius);
            }

            // Scrolling only changes the origin; reuse the shaped text until its inputs change
            let mut layout = Vec::new();
            style.font_weight = FontWeight::BOLD;
            style.color = cx.theme().foreground.opacity(1. / offsets.len() as f32);
            let font_size = window.rem_size() * (2. * scale);
            let run = style.to_run(original.len());
            if let Ok(lines) = window.text_system().shape_text(
                original.clone(),
                font_size,
                &[run],
                Some(bounds.size.width),
                None,
            ) {
                for line in lines {
                    layout.push((line, font_size * 1.2, px(0.)));
                }
            }
            if let Some(translation) = &translation {
                style.font_weight = FontWeight::NORMAL;
                style.color = cx
                    .theme()
                    .muted_foreground
                    .opacity(1. / offsets.len() as f32);
                let font_size = window.rem_size() * (1.125 * scale);
                let run = style.to_run(translation.len());
                if let Ok(lines) = window.text_system().shape_text(
                    translation.clone(),
                    font_size,
                    &[run],
                    Some(bounds.size.width),
                    None,
                ) {
                    for (ix, line) in lines.into_iter().enumerate() {
                        layout.push((
                            line,
                            font_size * 1.4,
                            if ix == 0 {
                                window.rem_size() * 0.25
                            } else {
                                px(0.)
                            },
                        ));
                    }
                }
            }
            let layout = Rc::new(layout);
            cache.update(cx, |cache, _| {
                cache.key = Some(key);
                cache.lines = layout.clone();
            });
            (layout, offsets, gpu_radius)
        },
        move |bounds, (layout, offsets, gpu_radius), window, cx| {
            let height = layout
                .iter()
                .map(|(line, height, gap)| line.size(*height).height + *gap)
                .fold(px(0.), |sum, height| sum + height);

            let mut origin =
                bounds.origin + point(px(0.), (bounds.size.height - height).max(px(0.)) * 0.5);

            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let paint = |window: &mut Window| {
                    for (line, line_height, gap) in layout.iter() {
                        origin.y += *gap;
                        for offset in &offsets {
                            line.paint(
                                origin + *offset,
                                *line_height,
                                TextAlign::Left,
                                Some(bounds),
                                window,
                                cx,
                            )
                            .expect("paint lyric glyphs");
                        }
                        origin.y += line.size(*line_height).height;
                    }
                };
                #[cfg(target_os = "windows")]
                window.with_glyph_blur(gpu_radius, paint);
                #[cfg(not(target_os = "windows"))]
                {
                    assert_eq!(gpu_radius, px(0.), "GPU glyph blur requires Windows");
                    paint(window);
                }
            });
        },
    )
    .size_full()
}

// Three mirrored pairs keep the blur centered without drawing thirteen copies
const BLUR_SAMPLES: [(f32, f32); 3] = [
    (1.893_018_5, 0.),
    (-0.868_185_3, 0.795_329_2),
    (0.052_792_35, -0.601_544_74),
];

fn blur_offsets(radius: Pixels) -> Vec<Point<Pixels>> {
    if radius < px(0.2) {
        return vec![point(px(0.), px(0.))];
    }
    let mut offsets = Vec::with_capacity(7);
    offsets.push(point(px(0.), px(0.)));
    for (x, y) in BLUR_SAMPLES {
        let offset = point(radius * x, radius * y);
        offsets.push(offset);
        offsets.push(-offset);
    }
    offsets
}

pub(super) fn scroll_ease(progress: f32) -> f32 {
    let t = progress.clamp(0., 1.);
    // Let the scroll settle like a spring, without overshooting the target
    (1. - (1. + 8. * t) * (-8. * t).exp()) / (1. - 9. * (-8.0f32).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharp_text_has_one_pass_and_blur_is_symmetric_without_position_drift() {
        assert_eq!(blur_offsets(px(0.)), vec![point(px(0.), px(0.))]);
        for radius in [0.5, 1.5, 3.5] {
            let offsets = blur_offsets(px(radius));
            assert!(
                offsets.len() <= 7,
                "Keep the per-line blur draw budget bounded"
            );
            let sum = offsets
                .iter()
                .fold(point(px(0.), px(0.)), |sum, offset| sum + *offset);

            assert!(sum.x.abs() < px(0.001) && sum.y.abs() < px(0.001));
            assert!(offsets.iter().any(|offset| offset.x.abs() > px(0.1)));
            assert!(offsets.iter().any(|offset| offset.y.abs() > px(0.1)));
            assert!(
                offsets
                    .iter()
                    .all(|offset| offset.x.abs() < px(radius * 3.)
                        && offset.y.abs() < px(radius * 3.))
            );
        }
    }

    #[test]
    fn spring_scroll_settles_at_the_target_without_overshooting() {
        assert!(scroll_ease(-1.).abs() < 0.00001);
        assert!((scroll_ease(2.) - 1.).abs() < 0.00001);
        let mut previous = 0.;
        for frame in 0..=120 {
            let sample = scroll_ease(frame as f32 / 120.);
            assert!((0. ..=1.).contains(&sample));
            assert!(sample >= previous);
            previous = sample;
        }
    }
}
