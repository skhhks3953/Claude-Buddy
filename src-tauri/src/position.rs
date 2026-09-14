//! Per-display position memory (§6.5).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition};

use clawd_core::layout::Layout;

/// Distance from the screen edge for the default corner.
const MARGIN: f64 = 24.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SavedPosition {
    /// Physical, absolute across the whole desktop.
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PositionStore {
    /// Keyed on monitor identity, so each display remembers its own spot.
    monitors: BTreeMap<String, SavedPosition>,
    /// The display the pet was last left on.
    ///
    /// Without this, startup takes whichever monitor enumerates first and has
    /// any saved entry — so once two displays are remembered, dragging to the
    /// second one is undone on every launch.
    #[serde(default)]
    last_used: Option<String>,
}

/// Which corner to snap to, from the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// A monitor's identity.
///
/// Name alone is not enough — two identical external displays report the same
/// one — so the resolution and desktop origin are folded in.
pub fn monitor_key(monitor: &Monitor) -> String {
    let size = monitor.size();
    let pos = monitor.position();
    format!(
        "{}@{}x{}+{}+{}",
        monitor.name().map(String::as_str).unwrap_or("display"),
        size.width,
        size.height,
        pos.x,
        pos.y
    )
}

impl PositionStore {
    fn path(app: &AppHandle) -> Option<PathBuf> {
        app.path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join("positions.json"))
    }

    pub fn load(app: &AppHandle) -> Self {
        // A missing or corrupt file is not an error worth surfacing: the pet
        // simply starts in its default corner.
        Self::path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app: &AppHandle) {
        let Some(path) = Self::path(app) else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
        }
    }

    pub fn remember(&mut self, monitor: &Monitor, position: PhysicalPosition<i32>) {
        let key = monitor_key(monitor);
        self.monitors.insert(
            key.clone(),
            SavedPosition {
                x: position.x,
                y: position.y,
            },
        );
        self.last_used = Some(key);
    }

    pub fn forget_all(&mut self) {
        self.monitors.clear();
        self.last_used = None;
    }

    pub fn get(&self, monitor: &Monitor) -> Option<SavedPosition> {
        self.monitors.get(&monitor_key(monitor)).copied()
    }
}

/// Which display the window should open on, and where.
///
/// The monitor is chosen *before* the layout is built, because the DPI snap
/// depends on that monitor's scale factor and the clamp depends on the size
/// the snap produces. Deriving the layout from the primary display and then
/// clamping a position saved on a 2x secondary is how the pet ends up half
/// off-screen on every launch.
pub struct Placement<'a> {
    pub monitor: &'a Monitor,
    /// `None` means nothing is remembered for this display: use the corner.
    pub saved: Option<SavedPosition>,
}

/// Prefers the display the pet was last left on, then any other remembered
/// one, then the primary. A remembered display that is gone falls through.
pub fn startup_placement<'a>(
    store: &PositionStore,
    monitors: &'a [Monitor],
    primary: Option<&'a Monitor>,
) -> Option<Placement<'a>> {
    let last_used = store
        .last_used
        .as_deref()
        .and_then(|key| monitors.iter().find(|m| monitor_key(m) == key));

    if let Some(monitor) = last_used.or_else(|| monitors.iter().find(|m| store.get(m).is_some())) {
        return Some(Placement {
            saved: store.get(monitor),
            monitor,
        });
    }

    Some(Placement {
        monitor: primary.or_else(|| monitors.first())?,
        saved: None,
    })
}

impl Placement<'_> {
    /// Resolve to an actual position, clamped into visible bounds so a display
    /// that shrank, or a pet dragged mostly off-screen, does not strand it.
    pub fn resolve(&self, layout: &Layout) -> PhysicalPosition<i32> {
        match self.saved {
            Some(saved) => clamp_to(self.monitor, layout, saved.x, saved.y),
            None => corner_of(self.monitor, layout, Corner::BottomRight),
        }
    }
}

/// Clamp a position so the whole window stays inside a monitor's work area.
pub fn clamp_to(monitor: &Monitor, layout: &Layout, x: i32, y: i32) -> PhysicalPosition<i32> {
    let area = monitor.work_area();
    let (w, h) = layout.window_physical();

    let min_x = area.position.x as f64;
    let min_y = area.position.y as f64;
    let max_x = min_x + area.size.width as f64 - w;
    let max_y = min_y + area.size.height as f64 - h;

    PhysicalPosition::new(
        (x as f64).clamp(min_x, max_x.max(min_x)).round() as i32,
        (y as f64).clamp(min_y, max_y.max(min_y)).round() as i32,
    )
}

/// A corner of a monitor's work area, inset by the margin.
///
/// The margin is scaled, so the gap looks the same on every display rather
/// than shrinking to nothing at 2×.
pub fn corner_of(monitor: &Monitor, layout: &Layout, corner: Corner) -> PhysicalPosition<i32> {
    let area = monitor.work_area();
    let (w, h) = layout.window_physical();
    let margin = MARGIN * layout.scale();

    let left = area.position.x as f64 + margin;
    let top = area.position.y as f64 + margin;
    let right = area.position.x as f64 + area.size.width as f64 - w - margin;
    let bottom = area.position.y as f64 + area.size.height as f64 - h - margin;

    let (x, y) = match corner {
        Corner::TopLeft => (left, top),
        Corner::TopRight => (right, top),
        Corner::BottomLeft => (left, bottom),
        Corner::BottomRight => (right, bottom),
    };
    clamp_to(monitor, layout, x.round() as i32, y.round() as i32)
}

/// The monitor a window is currently on, falling back to the primary display
/// when the platform will not say.
pub fn monitor_for(
    window: &tauri::WebviewWindow,
) -> Option<Monitor> {
    window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
}
