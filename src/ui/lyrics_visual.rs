use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    ContentMask, FontWeight, IntoElement, Pixels, Point, SharedString, Styled, TextAlign, canvas,
    point, px,
};

/// Draw the current line once to keep it sharp
/// Nearby inactive lines get extra offset copies for the soft blur
pub(super) fn lyric_text(
    original: SharedString,
    translation: Option<SharedString>,
    blur: f32,
    scale: f32,
) -> impl IntoElement {
    canvas(
        move |bounds, window, cx| {
            let radius = if bounds.contains(&window.mouse_position()) {
                px(0.)
            } else {
                window.rem_size() * blur
            };

            let offsets = blur_offsets(radius);
            let mut layout = Vec::new();
            let mut style = window.text_style();
            style.font_weight = FontWeight::BOLD;
            style.color = cx.theme().foreground.opacity(1. / offsets.len() as f32);
            let font_size = window.rem_size() * (2. * scale);
            let run = style.to_run(original.len());
            if let Ok(lines) = window.text_system().shape_text(
                original.clone(),
                font_size,
                &[run],
                Some(bounds.size.width),
                Some(2),
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
                    Some(2),
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
            (layout, offsets)
        },
        move |bounds, (layout, offsets), window, cx| {
            let height = layout
                .iter()
                .map(|(line, height, gap)| line.size(*height).height + *gap)
                .fold(px(0.), |sum, height| sum + height);

            let mut origin =
                bounds.origin + point(px(0.), (bounds.size.height - height).max(px(0.)) * 0.5);

            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                for (line, line_height, gap) in layout {
                    origin.y += gap;
                    for offset in &offsets {
                        let _ = line.paint(
                            origin + *offset,
                            line_height,
                            TextAlign::Left,
                            Some(bounds),
                            window,
                            cx,
                        );
                    }
                    origin.y += line.size(line_height).height;
                }
            });
        },
    )
    .size_full()
}

fn blur_offsets(radius: Pixels) -> Vec<Point<Pixels>> {
    if radius < px(0.2) {
        return vec![point(px(0.), px(0.))];
    }
    let mut offsets = Vec::with_capacity(13);
    offsets.push(point(px(0.), px(0.)));
    for ix in 0..6 {
        let distance = (-2. * ((ix as f32 + 0.5) / 6.).ln()).sqrt();
        let angle = ix as f32 * 2.399963;
        let offset = point(
            radius * (distance * angle.cos()),
            radius * (distance * angle.sin()),
        );

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
