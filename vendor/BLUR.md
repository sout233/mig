# MIG glyph blur patch

The two crates here are gpui-pre and gpui-pre-windows 0.3.8 from crates.io.
Their original Apache-2.0 licenses are retained. Cargo patches these crates so
the application can use the same GPUI Kit components and state types.

The Windows DirectX renderer applies the 7×7 Gaussian kernel from
gpui-fusion's `gpui_effects/src/filters/shaders/subtree_blur.wgsl` to glyph
atlas textures. The source filter's MIT license is in LICENSE-gpui-effects.
The kernel uses `exp(-dot(offset, offset) / 4)` with sample offsets spaced
at radius / 3 and normalizes the accumulated weights.

This is a glyph filter, not the original whole-subtree capture API. It covers
ordinary text and color emoji, including translated lyric lines. The blur
radius follows the application's existing per-line animation. Sharp lines
use the original shader branch. It creates no offscreen surfaces, readbacks,
or additional text draws.

`Window::with_glyph_blur` scopes a logical-pixel radius to glyph painting.
The renderer expands glyph bounds before scene insertion so the blur tail
participates in clipping and draw ordering. Sampling outside the glyph's
atlas tile returns transparent pixels instead of neighboring glyphs.
Color emoji use alpha-weighted color accumulation to avoid dark fringes.
The sprite padding word is replaced by a float radius without changing the
GPU structure size; ordinary sprites and images have radius zero.

This patch implements advanced blur on Windows only. Settings explicitly
identify Windows support and disable the switch on other platforms; those
platforms keep the existing lightweight effect. On Windows, new and existing
libraries default to advanced blur. Disabling it is persisted in library.json
and selects the existing seven-copy lightweight effect immediately.

When upgrading GPUI, reapply the changes in `scene.rs`, `window.rs`, and
`shaders.hlsl`, then run the native advanced-blur and queue-menu checks.
