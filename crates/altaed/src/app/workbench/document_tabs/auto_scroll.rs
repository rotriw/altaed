//! One bounded timer per native tab drag. The lifetime is owned by GPUI's drag
//! payload, so dropping/cancelling it stops this task without keeping a tab alive.
use super::*;
use gpui::{Bounds, Point};
use std::{sync::Weak, time::Duration};

const SCROLL_TICK: Duration = Duration::from_millis(16);
const EDGE_WIDTH_PX: f32 = 32.;
const MAX_STEP_PX: f32 = 12.;

impl Ide {
    pub(super) fn start_tab_auto_scroll(
        &mut self,
        lifetime: Weak<TabDragLifetime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.tab_auto_scroll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(SCROLL_TICK).await;
                let keep_scrolling = this.update_in(cx, |this, window, cx| {
                    if lifetime.strong_count() == 0 || !cx.has_active_drag() || this.busy {
                        return false;
                    }
                    let offset = this.tabs_scroll.offset();
                    let maximum = this.tabs_scroll.max_offset().width;
                    let step = edge_scroll_step(this.tabs_scroll.bounds(), window.mouse_position());
                    let next = (offset.x + px(step)).clamp(-maximum, px(0.));
                    if next != offset.x {
                        this.tabs_scroll.set_offset(Point::new(next, offset.y));
                        cx.notify();
                    }
                    true
                });
                if !matches!(keep_scrolling, Ok(true)) {
                    break;
                }
            }
        }));
    }
}

fn edge_scroll_step(bounds: Bounds<Pixels>, pointer: Point<Pixels>) -> f32 {
    if !bounds.contains(&pointer) || bounds.size.width <= px(0.) {
        return 0.;
    }
    let edge = px(EDGE_WIDTH_PX).min(bounds.size.width / 2.);
    if pointer.x < bounds.left() + edge {
        MAX_STEP_PX * ((bounds.left() + edge - pointer.x) / edge)
    } else if pointer.x > bounds.right() - edge {
        -MAX_STEP_PX * ((pointer.x - (bounds.right() - edge)) / edge)
    } else {
        0.
    }
}
