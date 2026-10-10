use gpui_kit::base::motion::{Interpolate, Transition, transition};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{
    App, Background, Hsla, Rgba, Window, hsla, linear_color_stop, linear_gradient, rgb,
};
use palette::{FromColor, Oklch, Srgb};
use std::path::Path;
use std::time::Duration;

/// Keep the palette here so views can just use the theme colors
pub(crate) fn apply(dark: bool, cx: &mut App) {
    apply_cover(dark, None, cx);
}

pub(crate) fn apply_cover(dark: bool, cover_color: Option<u32>, cx: &mut App) {
    let font_size = Theme::global(cx).font_size;
    if Theme::global(cx).is_dark() != dark {
        Theme::change(
            if dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            None,
            cx,
        );
    }
    Theme::update(cx, |theme| {
        theme.font_size = font_size;
        theme.font_family = "Microsoft YaHei UI".into();
        if dark {
            theme.background = rgb(0x171e1c).into();
            theme.foreground = rgb(0xe5ece7).into();
            theme.sidebar = rgb(0x1c2521).into();
            theme.sidebar_foreground = theme.foreground;
            theme.muted = rgb(0x25342d).into();
            theme.muted_foreground = rgb(0xa0b4a9).into();
            theme.primary = rgb(0x8ec6a9).into();
            theme.primary_foreground = rgb(0x13271d).into();
            theme.border = rgb(0x34453b).into();
        } else {
            theme.background = rgb(0xfcfcf9).into();
            theme.foreground = rgb(0x243a31).into();
            theme.sidebar = rgb(0xf3f5f0).into();
            theme.sidebar_foreground = theme.foreground;
            theme.muted = rgb(0xeef3ea).into();
            theme.muted_foreground = rgb(0x68786f).into();
            theme.primary = rgb(0x386a53).into();
            theme.primary_foreground = rgb(0xffffff).into();
            theme.border = rgb(0xe1e7de).into();
        }
        if let Some(color) = cover_color {
            let color: Hsla = rgb(color).into();

            let saturation = if color.s < 0.08 {
                0.
            } else {
                color.s.clamp(0.25, 0.72)
            };

            let tint = if saturation == 0. { 0. } else { 1. };
            let mut primary = hsla(color.h, saturation, if dark { 0.70 } else { 0.38 }, 1.);
            // Yellow can get hard to read in light mode, so darken it if needed
            //
            // wait...is that ok for okhcl? gpt?
            while !dark && white_contrast(primary) < 4.5 && primary.l > 0.16 {
                primary.l -= 0.02;
            }
            theme.primary = primary;
            theme.primary_foreground = if dark {
                hsla(color.h, 0.10 * tint, 0.08, 1.)
            } else {
                rgb(0xffffff).into()
            };
            theme.background = hsla(color.h, 0.08 * tint, if dark { 0.10 } else { 0.985 }, 1.);
            theme.foreground = hsla(color.h, 0.10 * tint, if dark { 0.92 } else { 0.14 }, 1.);
            theme.sidebar = hsla(color.h, 0.15 * tint, if dark { 0.12 } else { 0.95 }, 1.);
            theme.sidebar_foreground = theme.foreground;
            theme.muted = hsla(color.h, 0.16 * tint, if dark { 0.16 } else { 0.94 }, 1.);
            theme.muted_foreground = hsla(color.h, 0.10 * tint, if dark { 0.68 } else { 0.42 }, 1.);
            theme.border = hsla(color.h, 0.15 * tint, if dark { 0.24 } else { 0.90 }, 1.);
        }
        let top = gradient_top(theme.background, theme.primary, dark);
        theme.foreground = tinted_ink(
            theme.primary,
            theme.background,
            top,
            dark,
            if dark { 0.91 } else { 0.30 },
            0.045,
        );
        theme.muted_foreground = tinted_ink(
            theme.primary,
            theme.background,
            top,
            dark,
            if dark { 0.76 } else { 0.46 },
            0.030,
        );
        theme.sidebar_foreground = theme.foreground;
        theme.ring = theme.primary;
        theme.input = theme.border;
        theme.popover = theme.background;
        theme.popover_foreground = theme.foreground;
        theme.secondary = theme.muted;
        theme.secondary_foreground = theme.foreground;
        theme.secondary_active = theme.primary.opacity(if dark { 0.18 } else { 0.10 });
        theme.selection = theme.primary.opacity(0.22);
        theme.list_active = theme.primary.opacity(0.10);
        theme.list_hover = theme.primary.opacity(0.06);
        theme.list_active_border = theme.primary;
        theme.link = theme.primary;
        theme.link_hover = theme.primary;
        theme.link_active = theme.primary;
        let hover: gpui_kit::Hsla = rgb(0x000000).opacity(if dark { 0.16 } else { 0.08 }).into();
        let pressed: gpui_kit::Hsla = rgb(0x000000).opacity(if dark { 0.24 } else { 0.14 }).into();
        theme.button_hover = hover;
        theme.button_active = pressed;
        theme.button = surface(theme.muted, dark);
        theme.button_foreground = theme.foreground;
        // Ghost buttons halve this opacity in dark mode, so account for that here
        theme.accent = rgb(0x000000).opacity(if dark { 0.32 } else { 0.08 }).into();
        theme.accent_foreground = theme.foreground;
        theme.button_primary = theme.primary;
        theme.button_primary_foreground = theme.primary_foreground;
        theme.button_primary_hover = theme.primary.opacity(0.9);
        theme.button_primary_active = theme.primary;
        theme.button_secondary = theme.muted;
        theme.button_secondary_foreground = theme.foreground;
        theme.button_secondary_hover = hover;
        theme.button_secondary_active = pressed;
        theme.slider_bar = theme.primary;
        theme.sidebar_accent = theme.muted;
        theme.sidebar_accent_foreground = theme.foreground;
    });
}

pub(crate) struct AnimatedBackground {
    pub gradient: Background,
    pub surface: Hsla,
}

#[derive(Clone, PartialEq)]
struct BackgroundColors {
    top: Hsla,
    bottom: Hsla,
}

impl Interpolate for BackgroundColors {
    fn interpolate(&self, target: &Self, progress: f32) -> Self {
        // Crossfade RGB so we do not pass through random hues on the way
        let blend = |from: Hsla, to: Hsla| -> Hsla {
            let from: Rgba = from.into();
            let to: Rgba = to.into();
            Rgba {
                r: from.r + (to.r - from.r) * progress,
                g: from.g + (to.g - from.g) * progress,
                b: from.b + (to.b - from.b) * progress,
                a: from.a + (to.a - from.a) * progress,
            }
            .into()
        };

        Self {
            top: blend(self.top, target.top),
            bottom: blend(self.bottom, target.bottom),
        }
    }
}

/// Use the same colors for the background and player bar so they fade together
pub(crate) fn background(window: &mut Window, cx: &mut App) -> AnimatedBackground {
    let theme = Theme::global(cx);
    let target = BackgroundColors {
        top: gradient_top(theme.background, theme.primary, theme.is_dark()),
        bottom: theme.background,
    };

    let colors = transition(
        "window-background-colors",
        target,
        Transition::new(Duration::from_millis(700)).ease(|t| t * t * (3. - 2. * t)),
        window,
        cx,
    );

    AnimatedBackground {
        gradient: linear_gradient(
            180.,
            linear_color_stop(colors.top, 0.),
            linear_color_stop(colors.bottom, 1.),
        ),
        surface: colors.bottom.opacity(0.75),
    }
}

fn gradient_top(background: Hsla, primary: Hsla, dark: bool) -> Hsla {
    background.blend(primary.opacity(if dark { 0.18 } else { 0.16 }))
}

fn surface(muted: Hsla, dark: bool) -> Hsla {
    muted.opacity(if dark { 0.45 } else { 0.35 })
}

/// Give inputs and buttons the same see-through background
pub(crate) fn control_surface(cx: &App) -> Hsla {
    let theme = Theme::global(cx);
    surface(theme.muted, theme.is_dark())
}

fn tinted_ink(
    seed: Hsla,
    background: Hsla,
    top: Hsla,
    dark: bool,
    lightness: f32,
    chroma: f32,
) -> Hsla {
    let seed: Rgba = seed.into();
    let seed = Oklch::from_color(Srgb::new(seed.r, seed.g, seed.b));
    let mut lightness = lightness;
    for _ in 0..32 {
        // Keep gray covers gray instead of making up a new tint
        let ink: Srgb<f32> = Srgb::from_color(Oklch::new(
            lightness,
            chroma.min(seed.chroma * 0.55),
            seed.hue,
        ));

        let ink: Hsla = Rgba {
            r: ink.red,
            g: ink.green,
            b: ink.blue,
            a: 1.,
        }
        .into();

        if contrast(ink, background).min(contrast(ink, top)) >= 4.5 {
            return ink;
        }
        lightness = (lightness + if dark { 0.02 } else { -0.02 }).clamp(0.08, 0.98);
    }
    if dark {
        rgb(0xffffff).into()
    } else {
        rgb(0x000000).into()
    }
}

fn contrast(a: Hsla, b: Hsla) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn white_contrast(color: Hsla) -> f32 {
    1.05 / (luminance(color) + 0.05)
}

fn luminance(color: Hsla) -> f32 {
    let color: Rgba = color.into();

    let linear = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// Decode the cover in the background so rendering does not get stuck
pub(crate) fn extract_cover(path: &Path) -> Option<u32> {
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;

    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader.decode().ok()?.thumbnail(128, 128).to_rgba8();

    let palette =
        color_thief::get_palette(pixels.as_raw(), color_thief::ColorFormat::Rgba, 5, 8).ok();

    let pack = |r: u8, g: u8, b: u8| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    if let Some(colors) = palette {
        let score = |ix: usize, color: &color_thief::Color| {
            let hsl: Hsla = rgb(pack(color.r, color.g, color.b)).into();
            hsl.s * 0.75 + (1. - (hsl.l - 0.5).abs() * 2.) * 0.15 + 0.10 / (ix + 1) as f32
        };

        if let Some((_, color)) = colors
            .iter()
            .enumerate()
            .max_by(|(ix, a), (iy, b)| score(*ix, a).total_cmp(&score(*iy, b)))
        {
            return Some(pack(color.r, color.g, color.b));
        }
    }
    // Color-thief can miss plain white covers; keep them neutral instead of falling back to green
    let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
    for pixel in pixels.pixels().filter(|pixel| pixel[3] >= 128) {
        r += u32::from(pixel[0]);
        g += u32::from(pixel[1]);
        b += u32::from(pixel[2]);
        count += 1;
    }
    (count > 0).then(|| pack((r / count) as u8, (g / count) as u8, (b / count) as u8))
}

pub(crate) fn cover_scrim() -> Hsla {
    rgb(0x000000).opacity(0.38).into()
}

pub(crate) fn cover_indicator() -> Hsla {
    rgb(0xffffff).into()
}

/// Keep the heart red even when the cover color changes
pub(crate) fn favorite_color() -> Hsla {
    rgb(0xe84060).into()
}

/// GPUI keeps the base alpha when blending
/// That hides hover colors on transparent buttons, so blend the alpha too
pub(crate) fn overlay_surface(base: Hsla, overlay: Hsla) -> Hsla {
    let base: Rgba = base.into();
    let overlay: Rgba = overlay.into();
    let base_weight = base.a * (1. - overlay.a);
    let alpha = overlay.a + base_weight;
    if alpha <= f32::EPSILON {
        return base.into();
    }
    Rgba {
        r: (overlay.r * overlay.a + base.r * base_weight) / alpha,
        g: (overlay.g * overlay.a + base.g * base_weight) / alpha,
        b: (overlay.b * overlay.a + base.b * base_weight) / alpha,
        a: alpha,
    }
    .into()
}

#[cfg(test)]
mod control_surface_tests {
    use super::*;

    #[test]
    fn transparent_buttons_keep_the_hover_and_press_overlay_visible() {
        for alpha in [0.08, 0.14, 0.16, 0.24] {
            let base = rgb(0xffffff).opacity(0.).into();
            let overlay = rgb(0x000000).opacity(alpha).into();
            let result: Rgba = overlay_surface(base, overlay).into();
            assert!((result.a - alpha).abs() < 0.0001);
            assert!(result.r < 0.0001 && result.g < 0.0001 && result.b < 0.0001);
        }
    }

    #[test]
    fn hover_composes_over_the_selected_surface() {
        let base = rgb(0x386a53).opacity(0.1).into();
        let overlay = rgb(0x000000).opacity(0.08).into();
        let result: Rgba = overlay_surface(base, overlay).into();
        assert!((result.a - 0.172).abs() < 0.0001);
        assert!(result.g > result.r && result.g > result.b);
    }
}
