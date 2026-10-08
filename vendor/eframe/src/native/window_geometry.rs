//! Creation-time bounds checks for OpenRadar's native windows.

use crate::egui::{Pos2, Rect, Vec2, ViewportBuilder};

pub(crate) fn fit_to_monitors(mut builder: ViewportBuilder, monitors: &[Rect]) -> ViewportBuilder {
    if builder.clamp_size_to_monitor_size == Some(false) || builder.fullscreen == Some(true) {
        return builder;
    }
    // The primary monitor is first. Preserve valid secondary-monitor placement,
    // including negative coordinates; recover missing/disconnected monitors there.
    let monitor = builder
        .position
        .and_then(|position| monitors.iter().find(|monitor| monitor.contains(position)))
        .or_else(|| monitors.first());
    let (Some(monitor), Some(size)) = (monitor, builder.inner_size) else {
        return builder;
    };
    // Leave room for the title bar/borders and desktop controls on decorated windows.
    let margin = if builder.decorations.unwrap_or(true) {
        Vec2::new(64.0, 100.0)
    } else {
        Vec2::ZERO
    };
    let available = (monitor.size() - margin).max(Vec2::splat(1.0));
    let size = size.min(available);
    let position = builder
        .position
        .unwrap_or_else(|| monitor.center() - size * 0.5);
    let max_position = monitor.min + (monitor.size() - size - margin).max(Vec2::ZERO);
    builder.position = Some(Pos2::new(
        position.x.clamp(monitor.left(), max_position.x),
        position.y.clamp(monitor.top(), max_position.y),
    ));
    builder.inner_size = Some(size);
    builder.min_inner_size = builder.min_inner_size.map(|minimum| minimum.min(size));
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_windows_fit_small_and_secondary_monitors() {
        let primary = Rect::from_min_size(Pos2::ZERO, Vec2::new(1024.0, 768.0));
        let secondary = Rect::from_min_size(Pos2::new(-1280.0, 0.0), Vec2::new(1280.0, 720.0));
        let monitors = [primary, secondary];
        let panel = fit_to_monitors(
            ViewportBuilder::default()
                .with_inner_size([1900.0, 644.0])
                .with_min_inner_size([380.0, 400.0]),
            &monitors,
        );
        assert_eq!(panel.inner_size, Some(Vec2::new(960.0, 644.0)));
        let rect = Rect::from_min_size(panel.position.unwrap(), panel.inner_size.unwrap());
        assert!(primary.contains_rect(rect));
        for (position, expected) in [
            (Pos2::new(1000.0, 740.0), Pos2::new(584.0, 636.0)),
            (Pos2::new(8000.0, 8000.0), Pos2::new(584.0, 636.0)),
            (Pos2::new(-1200.0, 30.0), Pos2::new(-1200.0, 30.0)),
            (Pos2::new(-100.0, 700.0), Pos2::new(-440.0, 588.0)),
        ] {
            let window = fit_to_monitors(
                ViewportBuilder::default()
                    .with_position(position)
                    .with_inner_size([440.0, 132.0])
                    .with_decorations(false),
                &monitors,
            );
            assert_eq!(window.position, Some(expected));
            assert!(monitors.iter().any(|monitor| {
                monitor.contains_rect(Rect::from_min_size(expected, window.inner_size.unwrap()))
            }));
        }
        // A high-DPI display is supplied in egui points, not physical pixels.
        let small = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(320.0, 240.0));
        let window = fit_to_monitors(
            ViewportBuilder::default()
                .with_inner_size([800.0, 800.0])
                .with_min_inner_size([380.0, 400.0]),
            &[small],
        );
        assert_eq!(window.inner_size, Some(Vec2::new(256.0, 140.0)));
        assert_eq!(window.min_inner_size, window.inner_size);
        assert!(small.contains_rect(Rect::from_min_size(
            window.position.unwrap(),
            window.inner_size.unwrap()
        )));
        let unchanged = ViewportBuilder::default()
            .with_position([8000.0, 8000.0])
            .with_inner_size([800.0, 800.0]);
        assert_eq!(
            fit_to_monitors(unchanged.clone(), &[]).position,
            unchanged.position
        );
    }
}
