//! Sprite and window geometry, including the DPI snap.
//!
//! This is pure arithmetic with no UI dependency, so it lives here rather than
//! in the shell — which means the one piece of §6.4 that is easy to get subtly
//! wrong is covered by tests that run anywhere.

/// The sprite's design size, in CSS pixels at 1×.
pub const BASE_SPRITE: f64 = 80.0;
/// The sprite is a 16-unit grid. Every snapped size is a multiple of this.
pub const GRID: f64 = 16.0;

/// The window is deliberately larger than the sprite, so the pulse ring
/// (`inset: -5px`), the badge and the label chip overflow harmlessly (§6.1).
pub const WINDOW_W_RATIO: f64 = 2.75;
pub const WINDOW_H_RATIO: f64 = 2.1;

/// Vertical inset of the sprite box inside the window.
///
/// Sized for the worst case rather than the resting pose: the pulse ring is
/// `inset: -5px` and scales to 1.28, which already reaches ~18px above the
/// sprite box, and `hop` lifts the whole pose another 14px on top of that.
/// At the prototype's 10px the ring was clipped by the window edge every time
/// Clawd celebrated.
///
/// **This must stay in step with the `padding-top` on `.clawd-root` in
/// `app.css`** — the shell derives the hit rectangle from it.
pub const SPRITE_Y_RATIO: f64 = 0.45;

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    /// Whether a point falls inside, with `margin` of slack on every edge.
    ///
    /// The slack is the hysteresis that stops the click-through boundary
    /// chattering as the cursor crosses it (§6.1).
    pub fn contains(&self, x: f64, y: f64, margin: f64) -> bool {
        x >= self.x - margin
            && x <= self.x + self.w + margin
            && y >= self.y - margin
            && y <= self.y + self.h + margin
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    scale: f64,
    sprite_physical: f64,
}

impl Layout {
    /// Snap the sprite to the nearest whole number of physical pixels per grid
    /// unit for this display (§6.4).
    ///
    /// On a 1.5× display an unsnapped 80px sprite is 120 physical pixels — 7.5
    /// per grid unit, so the "pixels" render at uneven widths. `clip-path`
    /// keeps the edges sharp but the grid stops being regular. Snapping trades
    /// a little size variation across monitors for an honest grid.
    pub fn for_scale(scale: f64) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let units = (BASE_SPRITE * scale / GRID).round().max(1.0);
        Self {
            scale,
            sprite_physical: units * GRID,
        }
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Sprite edge in physical pixels. Always a whole multiple of `GRID`.
    pub fn sprite_physical(&self) -> f64 {
        self.sprite_physical
    }

    /// Sprite edge in CSS pixels — what the webview is told to draw.
    pub fn sprite_css(&self) -> f64 {
        self.sprite_physical / self.scale
    }

    /// Window size in logical pixels, which is what Tauri wants.
    pub fn window_css(&self) -> (f64, f64) {
        let s = self.sprite_css();
        (s * WINDOW_W_RATIO, s * WINDOW_H_RATIO)
    }

    /// Window size in physical pixels, for clamping against a monitor.
    pub fn window_physical(&self) -> (f64, f64) {
        let (w, h) = self.window_css();
        (w * self.scale, h * self.scale)
    }

    /// Offset of the sprite box inside the window, in physical pixels.
    ///
    /// Horizontally the figure is centred, which is where the ratio comes
    /// from; vertically it matches the CSS padding.
    pub fn sprite_offset_physical(&self) -> (f64, f64) {
        let s = self.sprite_physical;
        (s * (WINDOW_W_RATIO - 1.0) / 2.0, s * SPRITE_Y_RATIO)
    }

    /// The hit region: a rectangle test, not a per-pixel mask (§6.1).
    pub fn hit_rect(&self, window_x: f64, window_y: f64) -> Rect {
        let (dx, dy) = self.sprite_offset_physical();
        Rect {
            x: window_x + dx,
            y: window_y + dy,
            w: self.sprite_physical,
            h: self.sprite_physical,
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::for_scale(1.0)
    }
}
