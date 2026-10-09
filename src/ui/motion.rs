use super::*;
use gpui_kit::base::motion::{Transition, transition};
use gpui_kit::component::Size;
use gpui_kit::component::button::{
    Button as NativeButton, ButtonCustomVariant, ButtonVariant, ButtonVariants,
};
use std::time::Instant;

#[derive(Default)]
struct Interaction {
    hovered: bool,
    pressed: bool,
    pulse_until: Option<Instant>,
}

/// Let the native button handle clicks and layout; we just add the motion
#[derive(IntoElement)]
pub(super) struct MotionButton {
    inner: NativeButton,
    id: ElementId,
    ghost: bool,
    selected: bool,
    open: bool,
    disabled: bool,
    loading: bool,
    quiet_surface: bool,
    cover_overlay: bool,
}

impl MotionButton {
    pub(super) fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            inner: NativeButton::new(id.clone()),
            id,
            ghost: false,
            selected: false,
            open: false,
            disabled: false,
            loading: false,
            quiet_surface: false,
            cover_overlay: false,
        }
    }

    pub(super) fn cover_overlay(mut self) -> Self {
        self.cover_overlay = true;
        self
    }

    pub(super) fn quiet_surface(mut self) -> Self {
        self.quiet_surface = true;
        self
    }

    pub(super) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.inner = self.inner.label(label);
        self
    }

    pub(super) fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.inner = self.inner.icon(icon.into());
        self
    }

    pub(super) fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        // ManagedTooltip would overwrite our hover animation
        // Use the native tooltip slot so both can work
        self.inner.interactivity().tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
        });
        self
    }

    pub(super) fn accessibility_label(mut self, text: impl Into<SharedString>) -> Self {
        self.inner = self.inner.accessibility_label(text);
        self
    }

    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.inner = self.inner.disabled(disabled);
        self
    }

    pub(super) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self.inner = self.inner.loading(loading);
        self
    }

    pub(super) fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.inner = self.inner.on_click(handler);
        self
    }
}

impl Styled for MotionButton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

impl ParentElement for MotionButton {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(children);
    }
}

impl ButtonVariants for MotionButton {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.ghost = matches!(variant, ButtonVariant::Ghost);
        self.inner = self.inner.with_variant(variant);
        self
    }
}

impl Sizable for MotionButton {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.inner = self.inner.with_size(size);
        self
    }
}

impl Selectable for MotionButton {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self.inner = self.inner.selected(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }

    fn open(mut self, open: bool) -> Self {
        self.open = open;
        self.inner = self.inner.open(open);
        self
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

impl RenderOnce for MotionButton {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.quiet_surface {
            let transparent = cx.theme().transparent;
            let style = ButtonCustomVariant::new(cx)
                .color(transparent)
                .hover(transparent)
                .active(transparent)
                .foreground(cx.theme().foreground);

            return self.inner.custom(style).bg(transparent).into_any_element();
        }
        if self.disabled || self.loading {
            return self.inner.into_any_element();
        }

        let state_id = ElementId::NamedChild(self.id.clone().into(), "motion-pointer".into());
        let state = window.use_keyed_state(state_id, cx, |_, _| Interaction::default());

        let pointer = state.read(cx);
        let pulse = pointer
            .pulse_until
            .is_some_and(|until| Instant::now() < until);

        let pressed = pointer.pressed || pulse;
        let hovered = pointer.hovered;

        if pulse && !cx.reduce_motion() {
            window.request_animation_frame();
        }

        if self.cover_overlay {
            let opacity = transition(
                (self.id.clone(), "cover-overlay"),
                if hovered { 1.0_f32 } else { 0.0 },
                Transition::new(Duration::from_millis(160)),
                window,
                cx,
            );

            self.inner = self
                .inner
                .overflow_hidden()
                .rounded(cx.theme().radius_lg)
                .child(
                    h_flex()
                        .absolute()
                        .inset_0()
                        .justify_center()
                        .bg(crate::theme::cover_scrim())
                        .opacity(opacity)
                        .child(
                            Icon::new(gpui_kit::assets::IconName::ChevronUp)
                                .size_6()
                                .text_color(crate::theme::cover_indicator()),
                        ),
                );
        }
        let theme = cx.theme();

        let idle = if self.selected || self.open {
            theme.secondary_active
        } else if self.ghost {
            theme.transparent
        } else {
            theme.button
        };

        let target = if pressed {
            crate::theme::overlay_surface(idle, theme.button_active)
        } else if hovered {
            crate::theme::overlay_surface(idle, theme.button_hover)
        } else {
            idle
        };

        let background = transition(
            (self.id.clone(), "background"),
            target,
            Transition::new(Duration::from_millis(if pressed { 90 } else { 160 })),
            window,
            cx,
        );

        let pressure = transition(
            (self.id.clone(), "pressure"),
            if pressed && !cx.reduce_motion() {
                1.0_f32
            } else {
                0.0
            },
            Transition::new(Duration::from_millis(if pressed { 80 } else { 180 })),
            window,
            cx,
        );

        let style = ButtonCustomVariant::new(cx)
            .color(background)
            .hover(background)
            .active(background)
            .foreground(cx.theme().foreground);

        self.inner = self
            .inner
            .custom(style)
            .bg(background)
            .relative()
            .top(px(pressure * 0.8))
            .opacity(1. - pressure * 0.06);

        if !self.ghost {
            self.inner = self.inner.border_1().border_color(cx.theme().input);
        }
        let hover = state.clone();
        let down = state.clone();
        let up = state.clone();
        let outside = state.clone();
        let key_down = state.clone();

        self.inner
            .on_hover(move |hovered, _, cx| {
                hover.update(cx, |state, cx| {
                    state.hovered = *hovered;
                    if !hovered {
                        state.pressed = false;
                    }
                    cx.notify();
                })
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                down.update(cx, |state, cx| {
                    state.pressed = true;
                    state.pulse_until = Some(Instant::now() + Duration::from_millis(90));
                    cx.notify();
                })
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                up.update(cx, |state, cx| {
                    state.pressed = false;
                    cx.notify();
                })
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                outside.update(cx, |state, cx| {
                    state.pressed = false;
                    cx.notify();
                })
            })
            .on_key_down(move |event, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    key_down.update(cx, |state, cx| {
                        state.pulse_until = Some(Instant::now() + Duration::from_millis(140));
                        cx.notify();
                    });
                }
            })
            .into_any_element()
    }
}

/// The input still handles focus and typing; only its background fades
#[derive(IntoElement)]
pub(super) struct MotionField<E: IntoElement + Styled + 'static> {
    inner: E,
    id: &'static str,
}

pub(super) fn field<E: IntoElement + Styled + 'static>(
    inner: E,
    id: &'static str,
) -> MotionField<E> {
    MotionField { inner, id }
}

impl<E: IntoElement + Styled + 'static> RenderOnce for MotionField<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            ElementId::NamedChild(ElementId::from(self.id).into(), "field-hover".into()),
            cx,
            |_, _| false,
        );

        let idle = crate::theme::control_surface(cx);

        let target = if *state.read(cx) {
            crate::theme::overlay_surface(idle, cx.theme().button_hover)
        } else {
            idle
        };

        let background = transition(
            (self.id, "field-surface"),
            target,
            Transition::new(Duration::from_millis(170)),
            window,
            cx,
        );

        div()
            .id(self.id)
            .w_full()
            .h_9()
            .on_hover(move |hovered, _, cx| {
                state.update(cx, |state, cx| {
                    *state = *hovered;
                    cx.notify();
                })
            })
            .child(self.inner.bg(background))
    }
}

/// Put the hover background on the whole row, and leave focus to the play button
#[derive(IntoElement)]
pub(super) struct MotionRow {
    inner: Stateful<Div>,
    id: ElementId,
    idle: Hsla,
}

pub(super) fn row(inner: Stateful<Div>, id: ElementId, idle: Hsla) -> MotionRow {
    MotionRow { inner, id, idle }
}

impl RenderOnce for MotionRow {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(
            ElementId::NamedChild(self.id.clone().into(), "row-hover".into()),
            cx,
            |_, _| false,
        );

        let target = if *state.read(cx) {
            crate::theme::overlay_surface(self.idle, cx.theme().button_hover)
        } else {
            self.idle
        };

        let background = transition(
            (self.id, "row-background"),
            target,
            Transition::new(Duration::from_millis(160)),
            window,
            cx,
        );

        self.inner.bg(background).on_hover(move |hovered, _, cx| {
            state.update(cx, |state, cx| {
                *state = *hovered;
                cx.notify();
            });
        })
    }
}
