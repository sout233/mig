use super::*;
use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::base::slider::{Slider as BaseSlider, SliderIndicator, SliderThumb, SliderTrack};

#[derive(Default)]
struct PointerState {
    hovered: bool,
    pressed: bool,
}

#[derive(IntoElement)]
pub(super) struct ProgressSlider {
    state: Entity<SliderState>,
    id: &'static str,
    disabled: bool,
}

impl ProgressSlider {
    pub(super) fn new(state: &Entity<SliderState>, id: &'static str) -> Self {
        Self {
            state: state.clone(),
            id,
            disabled: false,
        }
    }

    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for ProgressSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pointer = window.use_keyed_state(self.id, cx, |_, _| PointerState::default());
        let visible = !self.disabled && {
            let state = pointer.read(cx);
            state.hovered || state.pressed
        };

        let opacity = transition(
            (self.id, "thumb-opacity"),
            if visible { 1.0_f32 } else { 0.0 },
            Transition::new(Duration::from_millis(130)),
            window,
            cx,
        );

        let percentage = self.state.read(cx).percentage().end;
        let bar = cx.theme().tokens.slider_bar;
        let hover = pointer.clone();
        let down = pointer.clone();
        let up = pointer.clone();
        div()
            .id(self.id)
            .w_full()
            .h_6()
            .on_hover(move |hovered, _, cx| {
                hover.update(cx, |state, cx| {
                    state.hovered = *hovered;
                    cx.notify();
                });
            })
            .capture_any_mouse_down(move |event, _, cx| {
                if event.button == MouseButton::Left {
                    down.update(cx, |state, cx| {
                        state.pressed = true;
                        cx.notify();
                    });
                }
            })
            .capture_any_mouse_up(move |event, _, cx| {
                if event.button == MouseButton::Left {
                    up.update(cx, |state, cx| {
                        state.pressed = false;
                        cx.notify();
                    });
                }
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                pointer.update(cx, |state, cx| {
                    state.pressed = false;
                    cx.notify();
                });
            })
            .child(
                BaseSlider::new(&self.state)
                    .disabled(self.disabled)
                    .flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .child(
                        SliderTrack::new(&self.state)
                            .disabled(self.disabled)
                            .flex()
                            .w_full()
                            .h_6()
                            .items_center()
                            .child(
                                SliderIndicator::new(&self.state)
                                    .relative()
                                    .w_full()
                                    .h_1p5()
                                    .rounded_full()
                                    .bg(bar.opacity(0.2))
                                    .child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .top_0()
                                            .h_full()
                                            .w(relative(percentage))
                                            .rounded_full()
                                            .bg(bar),
                                    )
                                    .child(
                                        SliderThumb::new(&self.state)
                                            .disabled(self.disabled)
                                            .absolute()
                                            .left(relative(percentage))
                                            .top(rems(-0.3125))
                                            .ml(rems(-0.5))
                                            .size_4()
                                            .rounded_full()
                                            .border_1()
                                            .border_color(bar.opacity(0.5))
                                            .bg(cx.theme().tokens.slider_thumb)
                                            .opacity(opacity),
                                    ),
                            ),
                    ),
            )
    }
}
