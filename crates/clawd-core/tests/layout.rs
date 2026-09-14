//! The DPI snap and the hit rectangle (§6.1, §6.4).

use clawd_core::layout::{Layout, BASE_SPRITE, GRID, SPRITE_Y_RATIO, WINDOW_W_RATIO};

#[test]
fn one_x_is_the_design_size_untouched() {
    let l = Layout::for_scale(1.0);
    assert_eq!(l.sprite_physical(), 80.0);
    assert_eq!(l.sprite_css(), 80.0);
}

/// The case the spec calls out: 80 at 1×, 112 or 128 at 1.5× — never 120,
/// which would be 7.5 physical pixels per grid unit.
#[test]
fn one_and_a_half_x_never_lands_on_120() {
    let l = Layout::for_scale(1.5);
    assert_ne!(l.sprite_physical(), 120.0);
    assert!(matches!(l.sprite_physical() as u32, 112 | 128));
}

#[test]
fn every_scale_yields_a_whole_number_of_pixels_per_grid_unit() {
    // Every scale factor a real display is likely to report, and then some.
    for step in 1..=40 {
        let scale = step as f64 * 0.1;
        let l = Layout::for_scale(scale);
        let physical = l.sprite_physical();
        assert_eq!(
            physical % GRID,
            0.0,
            "scale {scale} gave {physical} physical px, not a multiple of {GRID}"
        );
        assert!(physical >= GRID, "scale {scale} collapsed the sprite");
    }
}

/// Slight size variation across monitors is the price of an honest grid, but
/// it has to stay slight: half a grid unit is the most rounding can ever cost.
#[test]
fn the_snapped_size_stays_close_to_the_design_size() {
    for step in 10..=40 {
        let scale = step as f64 * 0.1;
        let l = Layout::for_scale(scale);
        assert!(
            (l.sprite_physical() - BASE_SPRITE * scale).abs() <= GRID / 2.0,
            "scale {scale} snapped further than half a grid unit"
        );
        assert!(
            (l.sprite_css() - BASE_SPRITE).abs() <= GRID / 2.0 / scale + 1e-9,
            "scale {scale} drifted to {}px",
            l.sprite_css()
        );
    }
}

/// Below about 0.1× the snap would round to nothing, so the sprite is floored
/// at one grid unit. No real display reports that, but a zero-sized window is
/// not a failure mode worth leaving open.
#[test]
fn the_sprite_is_floored_at_one_grid_unit() {
    assert_eq!(Layout::for_scale(0.05).sprite_physical(), GRID);
}

/// A nonsense scale factor must not produce a zero-sized or infinite window.
#[test]
fn a_bad_scale_factor_falls_back_rather_than_exploding() {
    for bad in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        let l = Layout::for_scale(bad);
        assert_eq!(l.sprite_physical(), 80.0, "{bad} was not rejected");
    }
}

#[test]
fn the_window_leaves_room_for_the_ring_badge_and_chip() {
    let l = Layout::for_scale(1.0);
    let (w, h) = l.window_css();
    // The ring sits at inset -5px and the badge overhangs by 2px; the chip is
    // wider than the sprite and sits below it.
    assert!(w > l.sprite_css() + 20.0);
    assert!(h > l.sprite_css() + 20.0);
}

#[test]
fn the_sprite_is_centred_horizontally_in_the_window() {
    let l = Layout::for_scale(2.0);
    let (dx, _) = l.sprite_offset_physical();
    let (window_w, _) = l.window_physical();
    let right_gap = window_w - dx - l.sprite_physical();
    assert!((dx - right_gap).abs() < 0.001, "left {dx} right {right_gap}");
    assert_eq!(dx, l.sprite_physical() * (WINDOW_W_RATIO - 1.0) / 2.0);
}

/// The regression this exists for: at the prototype's 10px top inset, the
/// pulse ring was clipped by the window edge on every celebratory hop.
///
/// The ring is `inset: -5px` and scales to 1.28, and `hop` lifts the pose a
/// further 14px, so the worst case reaches well above the sprite box.
#[test]
fn the_window_has_room_for_the_ring_at_full_pulse_mid_hop() {
    let l = Layout::for_scale(1.0);
    let sprite = l.sprite_css();

    let ring_box = sprite + 10.0; // inset: -5px on each side
    let overshoot_above = (ring_box * 1.28 - sprite) / 2.0;
    let hop_lift = 14.0;
    let needed = overshoot_above + hop_lift;

    let inset_above = sprite * SPRITE_Y_RATIO;
    assert!(
        inset_above >= needed,
        "ring clips: {inset_above}px of room above the sprite, {needed}px needed"
    );

    // And the same slack horizontally, where the window is far wider anyway.
    let (dx, _) = l.sprite_offset_physical();
    assert!(dx >= overshoot_above);
}

/// Below the sprite sit the meter, the gap, and the chip.
#[test]
fn the_window_has_room_for_the_meter_and_the_chip() {
    let l = Layout::for_scale(1.0);
    let sprite = l.sprite_css();
    let (_, window_h) = l.window_css();

    let below = window_h - sprite * SPRITE_Y_RATIO - sprite;
    let meter_overhang = 6.0;
    let figure_gap = 15.0;
    let chip_height = 26.0;
    assert!(
        below >= meter_overhang + figure_gap + chip_height,
        "only {below}px below the sprite for the meter, gap and chip"
    );
}

#[test]
fn the_hit_rect_follows_the_window() {
    let l = Layout::for_scale(1.0);
    let rect = l.hit_rect(1000.0, 500.0);
    let (dx, dy) = l.sprite_offset_physical();

    assert!(rect.contains(1000.0 + dx + 1.0, 500.0 + dy + 1.0, 0.0));
    // The window's own top-left corner is not the pet — that area is where the
    // chip and ring overflow, and must stay click-through.
    assert!(!rect.contains(1000.0, 500.0, 0.0));
}

/// Without slack the boundary chatters as the cursor crosses it.
#[test]
fn hysteresis_widens_the_rect_on_every_edge() {
    let rect = Layout::for_scale(1.0).hit_rect(0.0, 0.0);
    let just_outside = (rect.x - 3.0, rect.y + 10.0);

    assert!(!rect.contains(just_outside.0, just_outside.1, 0.0));
    assert!(rect.contains(just_outside.0, just_outside.1, 4.0));
}
