//! Reveal the toolkit's actual focused control inside its nearest scroll owner.

use iced::advanced::widget::operation::{
    Focusable, Outcome, Scrollable, scrollable::AbsoluteOffset,
};
use iced::advanced::widget::{Id, Operation, operate};
use iced::{Rectangle, Task, Vector};

#[derive(Clone, Copy)]
struct Viewport {
    bounds: Rectangle,
    content: Rectangle,
    translation: Vector,
}

#[derive(Default)]
struct Measure {
    target: Option<Id>,
    scroll: bool,
    pending: Option<Viewport>,
    parents: Vec<Viewport>,
    focused: Option<(Rectangle, Option<Viewport>)>,
}

impl Operation<bool> for Measure {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<bool>)) {
        let parent = self.pending.take();
        if let Some(viewport) = parent {
            self.parents.push(viewport);
        }
        visit(self);
        if parent.is_some() {
            self.parents.pop();
        }
    }

    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        _: &mut dyn Scrollable,
    ) {
        self.pending = Some(Viewport {
            bounds,
            content,
            translation,
        });
    }

    fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, state: &mut dyn Focusable) {
        if state.is_focused() && self.target.as_ref().is_none_or(|target| Some(target) == id) {
            self.focused = Some((bounds, self.parents.last().copied()));
        }
    }

    fn finish(&self) -> Outcome<bool> {
        let Some((bounds, viewport)) = self.focused else {
            return Outcome::Some(false);
        };
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return Outcome::Some(false);
        }
        let Some(viewport) = viewport else {
            return Outcome::Some(true);
        };
        let visible = Rectangle {
            x: bounds.x - viewport.translation.x,
            y: bounds.y - viewport.translation.y,
            ..bounds
        };
        let dy = if visible.y < viewport.bounds.y {
            visible.y - viewport.bounds.y
        } else if visible.y + visible.height > viewport.bounds.y + viewport.bounds.height {
            visible.y + visible.height - viewport.bounds.y - viewport.bounds.height
        } else {
            0.0
        };
        let dx = if visible.x < viewport.bounds.x {
            visible.x - viewport.bounds.x
        } else if visible.x + visible.width > viewport.bounds.x + viewport.bounds.width {
            visible.x + visible.width - viewport.bounds.x - viewport.bounds.width
        } else {
            0.0
        };
        let fits =
            visible.width <= viewport.bounds.width && visible.height <= viewport.bounds.height;
        if dy.abs() < 0.5 && dx.abs() < 0.5 && fits {
            return Outcome::Some(true);
        }
        if self.scroll && fits {
            Outcome::Chain(Box::new(Reveal {
                viewport,
                dx,
                dy,
                target: self.target.clone(),
            }))
        } else {
            Outcome::Some(false)
        }
    }
}

struct Reveal {
    viewport: Viewport,
    dx: f32,
    dy: f32,
    target: Option<Id>,
}

impl Operation<bool> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<bool>)) {
        visit(self);
    }

    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        _: Vector,
        state: &mut dyn Scrollable,
    ) {
        if bounds == self.viewport.bounds && content == self.viewport.content {
            state.scroll_by(
                AbsoluteOffset {
                    x: self.dx,
                    y: self.dy,
                },
                bounds,
                content,
            );
        }
    }

    fn finish(&self) -> Outcome<bool> {
        Outcome::Chain(Box::new(Measure {
            target: self.target.clone(),
            ..Measure::default()
        }))
    }
}

pub(crate) fn reveal_focused(target: Option<Id>) -> Task<bool> {
    operate(Measure {
        target,
        scroll: true,
        ..Measure::default()
    })
}

#[cfg(test)]
#[path = "../../tests/gui/focus/reveal_tests.rs"]
mod tests;
