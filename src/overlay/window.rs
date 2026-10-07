use crate::config::{Config, GapSettings};
use eframe::egui::{Pos2, Vec2, ViewportBuilder, WindowLevel};
use std::time::Duration;

const RESIZE_SETTLE: Duration = Duration::from_millis(250);

/// Each gap owns geometry and a resize debounce independent of the radar.
pub(super) struct GapWindow {
    position: Pos2,
    scale: f32,
    pending_scale: Option<(f32, Duration)>,
}
impl GapWindow {
    pub fn new(settings: &GapSettings) -> Self {
        Self {
            position: Pos2::new(
                settings.window_x.unwrap_or(0.0),
                settings.window_y.unwrap_or(0.0),
            ),
            scale: settings.scale,
            pending_scale: None,
        }
    }
    pub fn set_position(&mut self, settings: &GapSettings) {
        self.position = Pos2::new(
            settings.window_x.unwrap_or(0.0),
            settings.window_y.unwrap_or(0.0),
        );
    }
    pub fn settle_scale(&mut self, requested: f32, now: Duration, pointer_down: bool) -> bool {
        if requested == self.scale {
            self.pending_scale = None;
            return false;
        }
        if self
            .pending_scale
            .is_none_or(|(scale, _)| scale != requested)
        {
            self.pending_scale = Some((requested, now));
        }
        let (_, changed_at) = self.pending_scale.unwrap();
        if !pointer_down && now.saturating_sub(changed_at) >= RESIZE_SETTLE {
            self.scale = requested;
            self.pending_scale = None;
            return true;
        }
        false
    }
    pub fn builder(
        &self,
        title: &str,
        visible: bool,
        editing: bool,
        size: Vec2,
    ) -> ViewportBuilder {
        native_builder(title, self.position, size * self.scale, visible, editing)
    }
}

fn native_builder(
    title: &str,
    position: Pos2,
    size: Vec2,
    visible: bool,
    editing: bool,
) -> ViewportBuilder {
    ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_position(position)
        .with_transparent(true)
        .with_decorations(editing)
        .with_resizable(false)
        .with_active(false)
        .with_taskbar(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_mouse_passthrough(!editing)
        .with_visible(visible)
}

/// Only explicit control-panel edits change the requested native position.
/// Observed OS positions belong in Config, never fed back into this builder.
pub(super) struct OverlayWindow {
    position: Pos2,
    size: f32,
    pending_size: Option<(f32, Duration)>,
}

impl OverlayWindow {
    pub fn new(config: &Config) -> Self {
        Self {
            position: Pos2::new(config.overlay_x, config.overlay_y),
            size: config.overlay_size.round(),
            pending_size: None,
        }
    }

    pub fn set_position(&mut self, config: &Config) {
        self.position = Pos2::new(config.overlay_x, config.overlay_y);
    }

    /// Preview sliders immediately, but resize the swapchain only after an idle
    /// interval with no mouse button held. No wall-clock sleeps or GPU calls.
    pub fn settle_size(&mut self, requested: f32, now: Duration, pointer_down: bool) -> bool {
        let requested = requested.round();
        if requested == self.size {
            self.pending_size = None;
            return false;
        }
        if self.pending_size.is_none_or(|(size, _)| size != requested) {
            self.pending_size = Some((requested, now));
        }
        let (_, changed_at) = self.pending_size.unwrap();
        if !pointer_down && now.saturating_sub(changed_at) >= RESIZE_SETTLE {
            self.size = requested;
            self.pending_size = None;
            return true;
        }
        false
    }

    pub fn builder(&self, visible: bool, editing: bool) -> ViewportBuilder {
        native_builder(
            "LFS OpenRadar · Radar",
            self.position,
            Vec2::splat(self.size),
            visible,
            editing,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::gap_style;
    use eframe::egui::ViewportCommand;

    #[test]
    fn gap_windows_keep_geometry_independent_and_debounce_only_their_own_scale() {
        let mut config = Config::default();
        config.prepare_gap_positions();
        let mut ahead = GapWindow::new(&config.gap_ahead);
        let behind = GapWindow::new(&config.gap_behind);
        let mut native_ahead = ahead.builder("Ahead", true, false, gap_style::AHEAD.panel.size);
        let mut native_behind = behind.builder("Behind", true, false, gap_style::BEHIND.panel.size);
        config.gap_ahead.window_x = Some(-1600.0);
        config.gap_ahead.window_y = Some(500.0);
        ahead.set_position(&config.gap_ahead);
        let (commands, recreate) =
            native_ahead.patch(ahead.builder("Ahead", true, false, gap_style::AHEAD.panel.size));
        assert!(matches!(
            commands.as_slice(),
            [ViewportCommand::OuterPosition(_)]
        ));
        assert!(!recreate);
        assert!(
            native_behind
                .patch(behind.builder("Behind", true, false, gap_style::BEHIND.panel.size))
                .0
                .is_empty()
        );
        assert!(!ahead.settle_scale(2.0, Duration::ZERO, true));
        assert!(!ahead.settle_scale(2.0, Duration::from_secs(1), true));
        assert!(
            native_ahead
                .patch(ahead.builder("Ahead", true, false, gap_style::AHEAD.panel.size))
                .0
                .is_empty()
        );
        assert!(ahead.settle_scale(2.0, Duration::from_secs(1), false));
        let (commands, recreate) =
            native_ahead.patch(ahead.builder("Ahead", true, false, gap_style::AHEAD.panel.size));
        assert!(matches!(
            commands.as_slice(),
            [ViewportCommand::InnerSize(_)]
        ));
        assert!(!recreate);
        assert_eq!(native_ahead.inner_size, Some(Vec2::new(460.0, 152.0)));
        assert_eq!(native_behind.inner_size, Some(Vec2::new(230.0, 76.0)));
        let (commands, recreate) =
            native_ahead.patch(ahead.builder("Ahead", true, true, gap_style::AHEAD.panel.size));
        assert!(matches!(
            commands.as_slice(),
            [
                ViewportCommand::Decorations(true),
                ViewportCommand::MousePassthrough(false)
            ]
        ));
        assert!(!recreate);
        let (commands, recreate) =
            native_ahead.patch(ahead.builder("Ahead", false, true, gap_style::AHEAD.panel.size));
        assert!(matches!(
            commands.as_slice(),
            [ViewportCommand::Visible(false)]
        ));
        assert!(!recreate);
    }

    #[test]
    fn slider_drag_does_not_resize_until_settled_and_released() {
        let config = Config::default();
        let mut window = OverlayWindow::new(&config);
        let mut previous = window.builder(true, true);
        for frame in 0..80 {
            let size = 220.0 + frame as f32 * 7.0;
            assert!(!window.settle_size(size, Duration::from_millis(frame * 16), true));
            let (commands, recreate) = previous.patch(window.builder(true, true));
            assert!(commands.is_empty());
            assert!(!recreate);
        }
        // Holding the slider still must not resize it while pressed.
        assert!(!window.settle_size(773.0, Duration::from_secs(3), true));
        assert!(window.settle_size(773.0, Duration::from_secs(3), false));
        let (commands, recreate) = previous.patch(window.builder(true, true));
        assert!(matches!(
            commands.as_slice(),
            [ViewportCommand::InnerSize(_)]
        ));
        assert!(!recreate);
        assert!(!window.settle_size(773.0, Duration::from_secs(4), false));
    }

    #[test]
    fn returning_to_original_size_cancels_pending_resize() {
        let config = Config::default();
        let mut window = OverlayWindow::new(&config);
        assert!(!window.settle_size(800.0, Duration::ZERO, true));
        assert!(!window.settle_size(config.overlay_size, Duration::from_secs(1), false));
        assert!(window.pending_size.is_none());
        assert_eq!(window.size, config.overlay_size);
    }

    #[test]
    fn position_mode_uses_title_bar_without_recreating_window() {
        let window = OverlayWindow::new(&Config::default());
        let mut previous = window.builder(true, false);
        let (commands, recreate) = previous.patch(window.builder(true, true));
        assert!(!recreate);
        assert!(matches!(
            commands.as_slice(),
            [
                ViewportCommand::Decorations(true),
                ViewportCommand::MousePassthrough(false)
            ]
        ));
        let (commands, recreate) = previous.patch(window.builder(true, false));
        assert!(!recreate);
        assert!(matches!(
            commands.as_slice(),
            [
                ViewportCommand::Decorations(false),
                ViewportCommand::MousePassthrough(true)
            ]
        ));
    }

    #[test]
    fn hiding_and_range_changes_preserve_native_geometry() {
        let mut config = Config::default();
        let mut window = OverlayWindow::new(&config);
        let mut previous = window.builder(true, true);
        let (commands, recreate) = previous.patch(window.builder(true, true));
        assert!(commands.is_empty());
        assert!(!recreate);
        let (commands, recreate) = previous.patch(window.builder(false, true));
        assert!(matches!(
            commands.as_slice(),
            [ViewportCommand::Visible(false)]
        ));
        assert!(!recreate);
        for side in [3.0, 12.0, 5.0] {
            config.side_m = side;
            // Simulate receiving a moved native window position.
            config.overlay_x += 20.0;
            config.overlay_y += 30.0;
            config.validate().unwrap();
            assert!(!window.settle_size(config.overlay_size, Duration::from_secs(5), false));
            let (commands, recreate) = previous.patch(window.builder(false, true));
            assert!(
                commands.is_empty(),
                "range / observed position must not move or resize the OS window"
            );
            assert!(!recreate);
        }
    }

    #[test]
    fn keyboard_size_edits_wait_for_idle_interval() {
        let mut window = OverlayWindow::new(&Config::default());
        assert!(!window.settle_size(500.0, Duration::ZERO, false));
        assert!(!window.settle_size(501.0, Duration::from_millis(240), false));
        assert!(!window.settle_size(501.0, Duration::from_millis(489), false));
        assert!(window.settle_size(501.0, Duration::from_millis(490), false));
    }

    #[test]
    fn explicit_position_edit_sends_one_move() {
        let mut config = Config::default();
        let mut window = OverlayWindow::new(&config);
        let mut previous = window.builder(true, true);
        config.overlay_x = 200.0;
        window.set_position(&config);
        let (commands, recreate) = previous.patch(window.builder(true, true));
        assert!(matches!(
            commands.as_slice(),
            [ViewportCommand::OuterPosition(_)]
        ));
        assert!(!recreate);
        assert!(previous.patch(window.builder(true, true)).0.is_empty());
    }
}
