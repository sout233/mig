use super::*;
use gpui_kit::component::scroll::ScrollableElement as _;
use std::{cell::RefCell, rc::Rc, time::Instant};

#[derive(Default)]
struct ScrollState {
    handle: UniformListScrollHandle,
    motion: Option<ScrollMotion>,
    before: Point<Pixels>,
}

struct ScrollMotion {
    from: Point<Pixels>,
    target: Point<Pixels>,
    shown: Point<Pixels>,
    started: Instant,
}

pub(super) trait ScrollTarget: Element + Styled + Sized + 'static {
    fn attach(self, handle: &UniformListScrollHandle, scrollbar: bool) -> AnyElement;

    fn smooth_scroll(self, id: &'static str) -> SmoothScroll<Self> {
        SmoothScroll {
            inner: self,
            id: id.into(),
            handle: None,
            scrollbar: false,
        }
    }

    fn smooth_scroll_with(
        self,
        id: &'static str,
        handle: &UniformListScrollHandle,
    ) -> SmoothScroll<Self> {
        SmoothScroll {
            inner: self,
            id: id.into(),
            handle: Some(handle.clone()),
            scrollbar: false,
        }
    }

    fn smooth_scrollbar(self, id: &'static str) -> SmoothScroll<Self> {
        SmoothScroll {
            inner: self,
            id: id.into(),
            handle: None,
            scrollbar: true,
        }
    }
}

impl ScrollTarget for Stateful<Div> {
    fn attach(mut self, handle: &UniformListScrollHandle, scrollbar: bool) -> AnyElement {
        let handle = handle.0.borrow().base_handle.clone();
        if !scrollbar {
            return self.track_scroll(&handle).into_any_element();
        }

        // Keep the same content sizing as the component library's scrollable
        let style = self.style();
        let root_style = StyleRefinement {
            size: style.size.clone(),
            min_size: style.min_size.clone(),
            max_size: style.max_size.clone(),
            flex_grow: style.flex_grow,
            flex_shrink: style.flex_shrink,
            flex_basis: style.flex_basis,
            align_self: style.align_self,
            ..Default::default()
        };
        style.overflow.y = Some(Overflow::Visible);
        let content = self.flex_none().h_auto().min_h_full();
        let area = div()
            .id("smooth-scroll-area")
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .lock_scroll_axis()
            .track_scroll(&handle)
            .child(content);

        div()
            .id("smooth-scroll-root")
            .size_full()
            .refine_style(&root_style)
            .overflow_hidden()
            .relative()
            .child(area)
            .vertical_scrollbar(&handle)
            .into_any_element()
    }
}

impl ScrollTarget for UniformList {
    fn attach(self, handle: &UniformListScrollHandle, _: bool) -> AnyElement {
        self.track_scroll(handle).into_any_element()
    }
}

#[derive(IntoElement)]
pub(super) struct SmoothScroll<E: ScrollTarget> {
    inner: E,
    id: ElementId,
    handle: Option<UniformListScrollHandle>,
    scrollbar: bool,
}

impl<E: ScrollTarget> Styled for SmoothScroll<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.inner.style()
    }
}

impl<E: ScrollTarget + ParentElement> ParentElement for SmoothScroll<E> {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(children);
    }
}

impl<E: ScrollTarget> RenderOnce for SmoothScroll<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state_id = ElementId::NamedChild(self.id.clone().into(), "wheel-state".into());
        let state = window.use_keyed_state(state_id, cx, |_, _| {
            Rc::new(RefCell::new(ScrollState::default()))
        });
        let state = state.read(cx).clone();
        let handle = self.handle.unwrap_or_else(|| state.borrow().handle.clone());
        SmoothArea {
            id: self.id,
            inner: self.inner.attach(&handle, self.scrollbar),
            handle: handle.0.borrow().base_handle.clone(),
            state,
        }
    }
}

struct SmoothArea {
    id: ElementId,
    inner: AnyElement,
    handle: ScrollHandle,
    state: Rc<RefCell<ScrollState>>,
}

impl IntoElement for SmoothArea {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

fn clamp_offset(offset: Point<Pixels>, max: Point<Pixels>) -> Point<Pixels> {
    point(
        offset.x.clamp(-max.x, px(0.)),
        offset.y.clamp(-max.y, px(0.)),
    )
}

impl Element for SmoothArea {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.inner.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let mut state = self.state.borrow_mut();
        let current = self.handle.offset();
        if let Some(motion) = state.motion.as_mut() {
            // A scrollbar drag or a programmatic jump takes over immediately
            if current != motion.shown || cx.reduce_motion() {
                state.motion = None;
            } else {
                let t = (motion.started.elapsed().as_secs_f32() / 0.22).min(1.);
                let eased = 1. - (1. - t).powi(3);
                let target = clamp_offset(motion.target, self.handle.max_offset());
                let next = point(
                    motion.from.x + (target.x - motion.from.x) * eased,
                    motion.from.y + (target.y - motion.from.y) * eased,
                );
                motion.target = target;
                motion.shown = clamp_offset(next, self.handle.max_offset());
                self.handle.set_offset(motion.shown);
                if t < 1. {
                    window.request_animation_frame();
                } else {
                    state.motion = None;
                }
            }
        }
        drop(state);

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.inner.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let hitbox = hitbox.clone();
        let handle = self.handle.clone();
        let state = self.state.clone();
        let view = window.current_view();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if !hitbox.should_handle_scroll(window) {
                return;
            }

            let mut state = state.borrow_mut();
            if phase == DispatchPhase::Capture {
                state.before = handle.offset();
                if event.delta.precise() || cx.reduce_motion() {
                    state.motion = None;
                }
                return;
            }
            if event.delta.precise() || cx.reduce_motion() {
                return;
            }

            // Let the native area choose the axis, then animate the offset it requested
            let before = state.before;
            let after = handle.offset();
            let delta = after - before;
            let previous = state.motion.as_ref().map_or(before, |motion| motion.target);
            let target = clamp_offset(previous + delta, handle.max_offset());
            handle.set_offset(before);
            if target == previous {
                return;
            }

            state.motion = Some(ScrollMotion {
                from: before,
                target,
                shown: before,
                started: Instant::now(),
            });
            // Input events run outside the paint stack; notify the view captured above
            cx.notify(view);
            cx.stop_propagation();
        });
        self.inner.paint(window, cx);
    }
}

#[cfg(all(test, feature = "ui-verify"))]
mod tests {
    use super::ScrollTarget;
    use gpui_kit as gpui;
    use gpui_kit::{
        Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta,
        ScrollWheelEvent, StatefulInteractiveElement, Styled, TestAppContext,
        UniformListScrollHandle, VisualTestContext, Window, div, point, px, uniform_list,
    };

    struct ScrollView {
        kind: usize,
        handle: UniformListScrollHandle,
        renders: usize,
    }

    impl Render for ScrollView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders += 1;
            let content = if self.kind == 0 {
                uniform_list("test-list", 50, |range, _, _| {
                    range.map(|_| div().h(px(40.))).collect::<Vec<_>>()
                })
                .size_full()
                .smooth_scroll_with("test-wheel", &self.handle)
                .into_any_element()
            } else {
                let area = div()
                    .id("test-content")
                    .size_full()
                    .overflow_y_scroll()
                    .children((0..50).map(|_| div().h(px(40.)).flex_shrink_0()));
                if self.kind == 1 {
                    area.smooth_scroll_with("test-wheel", &self.handle)
                        .into_any_element()
                } else {
                    area.smooth_scrollbar("test-wheel").into_any_element()
                }
            };
            div().w(px(120.)).h(px(120.)).child(content)
        }
    }

    #[gpui::test]
    fn coarse_wheel_events_notify_the_view_without_requiring_a_paint_stack(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        for kind in 0..3 {
            let (view, cx) = cx.add_window_view(|_, _| ScrollView {
                kind,
                handle: UniformListScrollHandle::new(),
                renders: 0,
            });
            let cx: &mut VisualTestContext = cx;
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            let before = view.read_with(cx, |view, _| view.renders);
            for delta in [-3., -3., 3.] {
                cx.simulate_event(ScrollWheelEvent {
                    position: point(px(20.), px(20.)),
                    delta: ScrollDelta::Lines(point(0., delta)),
                    ..Default::default()
                });
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
            }
            assert!(view.read_with(cx, |view, _| view.renders) > before);
        }
    }
}
