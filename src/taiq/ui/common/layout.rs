//! Viewport sizing shared by both desktop renderers.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneLayout {
    pub left: f32,
    pub center: f32,
    pub right: f32,
    pub show_left: bool,
    pub show_right: bool,
}

impl PaneLayout {
    pub fn fit(viewport: f32, rail: f32, preferred_left: f32, preferred_right: f32) -> Self {
        let width = (viewport - rail).max(0.0);
        let show_left = width >= 580.0;
        let show_right = width >= 860.0;
        let splitters = (show_left as u8 as f32 + show_right as u8 as f32) * 6.0;
        let usable = (width - splitters).max(0.0);
        let left = if show_left {
            preferred_left.clamp(170.0, (usable * 0.30).max(170.0))
        } else {
            0.0
        };
        let right = if show_right {
            preferred_right.clamp(260.0, (usable * 0.38).max(260.0))
        } else {
            0.0
        };
        let center = (usable - left - right).max(0.0);
        Self {
            left,
            center,
            right,
            show_left,
            show_right,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PaneLayout;

    #[test]
    fn panes_always_fit_viewport() {
        for width in [320.0, 650.0, 900.0, 1440.0] {
            let p = PaneLayout::fit(width, 48.0, 600.0, 750.0);
            let splitters = (p.show_left as u8 as f32 + p.show_right as u8 as f32) * 6.0;
            assert!((p.left + p.center + p.right + splitters + 48.0 - width).abs() < 0.01);
        }
    }
}
