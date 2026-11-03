//! Window-local layout preferences. These never become code-document metadata.

pub const ACTIVITY_BAR_WIDTH: f32 = 44.;
pub const RESIZE_HANDLE: f32 = 1.0;
pub const RESIZE_HIT_AREA: f32 = 5.0;
pub const STATUS_HEIGHT: f32 = 24.0;
const MIN_EDITOR_WIDTH: f32 = 360.0;
const MIN_EDITOR_HEIGHT: f32 = 180.0;
const TAB_BAR_HEIGHT: f32 = 34.0;
pub const EDITOR_ACTIONS_HEIGHT: f32 = 34.0;
const VERTICAL_CHROME: f32 = TAB_BAR_HEIGHT + EDITOR_ACTIONS_HEIGHT + STATUS_HEIGHT + RESIZE_HANDLE;
const EMPTY_MESSAGES_HEIGHT: f32 = 76.0;

#[derive(Clone, Copy, Debug)]
pub enum Panel {
    Documents,
    Context,
    Output,
    Messages,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextTab {
    Tests,
    Problem,
}

pub struct WorkbenchLayout {
    pub documents_visible: bool,
    pub context_visible: bool,
    pub output_visible: bool,
    pub messages_visible: bool,
    messages_height: f32,
    messages_resized: bool,
    pub context_tab: ContextTab,
    documents_width: f32,
    context_width: f32,
    output_height: f32,
}

impl Default for WorkbenchLayout {
    fn default() -> Self {
        Self {
            documents_visible: false,
            context_visible: true,
            output_visible: false,
            messages_visible: true,
            messages_height: 150.0,
            messages_resized: false,
            context_tab: ContextTab::Tests,
            documents_width: 220.0,
            context_width: 520.0,
            output_height: 150.0,
        }
    }
}

impl WorkbenchLayout {
    pub fn messages_height(&self, has_output: bool) -> f32 {
        if has_output || self.messages_resized {
            self.messages_height
        } else {
            EMPTY_MESSAGES_HEIGHT
        }
    }
    pub fn toggle(&mut self, panel: Panel) {
        let visible = match panel {
            Panel::Documents => &mut self.documents_visible,
            Panel::Context => &mut self.context_visible,
            Panel::Output => &mut self.output_visible,
            Panel::Messages => &mut self.messages_visible,
        };
        *visible = !*visible;
    }

    pub fn requested_size(&self, panel: Panel) -> f32 {
        match panel {
            Panel::Documents => self.documents_width,
            Panel::Context => self.context_width,
            Panel::Output => self.output_height,
            Panel::Messages => self.messages_height,
        }
    }

    pub fn set_size(&mut self, panel: Panel, value: f32) {
        if !value.is_finite() {
            return;
        }
        match panel {
            Panel::Documents => self.documents_width = value.clamp(150.0, 360.0),
            Panel::Context => self.context_width = value.clamp(220.0, 720.0),
            Panel::Output => self.output_height = value.clamp(90.0, 420.0),
            Panel::Messages => {
                self.messages_height = value.clamp(66.0, 300.0);
                self.messages_resized = true;
            }
        }
    }

    pub fn reset_size(&mut self, panel: Panel) {
        self.set_size(panel, Self::default().requested_size(panel));
        if matches!(panel, Panel::Messages) {
            self.messages_resized = false;
        }
    }

    /// Derive visible sizes without overwriting the user's preferred sizes.
    pub fn sizes(&self, width: f32, height: f32) -> (f32, f32, f32) {
        let left = if self.documents_visible {
            self.documents_width
        } else {
            0.0
        };
        let right = if self.context_visible {
            self.context_width
        } else {
            0.0
        };
        let horizontal_budget =
            (width - ACTIVITY_BAR_WIDTH - MIN_EDITOR_WIDTH - 2.0 * RESIZE_HANDLE).max(0.0);
        let scale = (horizontal_budget / (left + right).max(1.0)).min(1.0);
        let bottom = if self.output_visible {
            self.output_height
                .min((height - VERTICAL_CHROME - MIN_EDITOR_HEIGHT).max(0.0))
        } else {
            0.0
        };
        // Round constrained panels inward; floating-point multiplication can
        // otherwise consume a fraction of the editor's minimum width.
        if scale < 1.0 {
            ((left * scale).floor(), (right * scale).floor(), bottom)
        } else {
            (left, right, bottom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_messages_are_compact_until_the_user_resizes_them() {
        let mut layout = WorkbenchLayout::default();
        assert_eq!(layout.messages_height(false), 76.);
        assert_eq!(layout.messages_height(true), 150.);
        layout.set_size(Panel::Messages, 123.);
        assert_eq!(layout.messages_height(false), 123.);
        layout.reset_size(Panel::Messages);
        assert_eq!(layout.messages_height(false), 76.);
    }

    #[test]
    fn narrow_window_preserves_editor_space_and_requested_sizes() {
        let mut layout = WorkbenchLayout {
            documents_visible: true,
            output_visible: true,
            ..Default::default()
        };
        layout.set_size(Panel::Documents, 360.0);
        layout.set_size(Panel::Context, 520.0);
        let (left, right, bottom) = layout.sizes(900.0, 500.0);
        assert!(900.0 - left - right - 2.0 * RESIZE_HANDLE >= MIN_EDITOR_WIDTH);
        assert!(500.0 - bottom - VERTICAL_CHROME >= MIN_EDITOR_HEIGHT);
        assert_eq!(layout.requested_size(Panel::Context), 520.0);
        layout.toggle(Panel::Context);
        assert_eq!(layout.sizes(900.0, 500.0).1, 0.0);
        layout.toggle(Panel::Context);
        assert_eq!(layout.sizes(1400.0, 800.0).1, 520.0);
    }
}
