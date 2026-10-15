//! Input-event plumbing: counters and last-known state for the events the
//! shell receives from the window system.
//!
//! Phase 1 scope is *plumbing only*: events arrive from the windowing
//! backend, are counted here, and are logged at debug level. There is no
//! hit testing, no DOM event dispatch, and no IME handling — those arrive
//! with the content pipeline (rendering-pipeline §6, Phase 3/4). Keeping
//! the accounting in a plain struct (no winit types) makes it unit-testable
//! without a display server.

use std::fmt;

/// Accumulated input statistics for one shell session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputStats {
    keyboard_presses: u64,
    keyboard_releases: u64,
    keyboard_synthetic_ignored: u64,
    pointer_moves: u64,
    pointer_button_presses: u64,
    pointer_button_releases: u64,
    last_pointer: Option<(i32, i32)>,
}

impl InputStats {
    /// Records a key press. Synthetic events (auto-repeat injected by the
    /// platform) are counted separately so tests can assert on real input.
    pub fn record_key_press(&mut self, synthetic: bool) {
        if synthetic {
            self.keyboard_synthetic_ignored += 1;
        } else {
            self.keyboard_presses += 1;
        }
    }

    /// Records a key release. Synthetic releases are ignored the same way
    /// as synthetic presses.
    pub fn record_key_release(&mut self, synthetic: bool) {
        if synthetic {
            self.keyboard_synthetic_ignored += 1;
        } else {
            self.keyboard_releases += 1;
        }
    }

    /// Records a pointer move. Coordinates are stored in whatever physical
    /// units the backend reported, clamped to `i32` range for storage.
    pub fn record_pointer_move(&mut self, x: f64, y: f64) {
        self.pointer_moves += 1;
        self.last_pointer = Some((clamp_i32(x), clamp_i32(y)));
    }

    /// Records a pointer button transition.
    pub fn record_pointer_button(&mut self, pressed: bool) {
        if pressed {
            self.pointer_button_presses += 1;
        } else {
            self.pointer_button_releases += 1;
        }
    }

    /// Count of non-synthetic key presses.
    pub fn keyboard_presses(&self) -> u64 {
        self.keyboard_presses
    }

    /// Count of non-synthetic key releases.
    pub fn keyboard_releases(&self) -> u64 {
        self.keyboard_releases
    }

    /// Count of synthetic key events observed and excluded.
    pub fn keyboard_synthetic_ignored(&self) -> u64 {
        self.keyboard_synthetic_ignored
    }

    /// Count of pointer moves.
    pub fn pointer_moves(&self) -> u64 {
        self.pointer_moves
    }

    /// Count of pointer button presses.
    pub fn pointer_button_presses(&self) -> u64 {
        self.pointer_button_presses
    }

    /// Count of pointer button releases.
    pub fn pointer_button_releases(&self) -> u64 {
        self.pointer_button_releases
    }

    /// Last pointer position, if any move was recorded.
    pub fn last_pointer(&self) -> Option<(i32, i32)> {
        self.last_pointer
    }
}

fn clamp_i32(value: f64) -> i32 {
    if value.is_nan() {
        0
    } else if value >= f64::from(i32::MAX) {
        i32::MAX
    } else if value <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        value as i32
    }
}

impl fmt::Display for InputStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "keys {}up/{}down ({} synthetic ignored), pointer {} moves (last {:?}), buttons \
             {}/{}",
            self.keyboard_releases,
            self.keyboard_presses,
            self.keyboard_synthetic_ignored,
            self.pointer_moves,
            self.last_pointer,
            self.pointer_button_presses,
            self.pointer_button_releases
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn counters_start_at_zero() {
        let stats = InputStats::default();
        assert_eq!(stats.keyboard_presses(), 0);
        assert_eq!(stats.pointer_moves(), 0);
        assert_eq!(stats.last_pointer(), None);
    }

    #[test]
    fn key_presses_are_counted_and_synthetic_separated() {
        let mut stats = InputStats::default();
        stats.record_key_press(false);
        stats.record_key_press(false);
        stats.record_key_release(false);
        stats.record_key_press(true);
        stats.record_key_release(true);
        assert_eq!(stats.keyboard_presses(), 2);
        assert_eq!(stats.keyboard_releases(), 1);
        assert_eq!(stats.keyboard_synthetic_ignored(), 2);
    }

    #[test]
    fn pointer_moves_track_last_position() {
        let mut stats = InputStats::default();
        stats.record_pointer_move(12.0, 34.0);
        stats.record_pointer_move(56.7, 89.9);
        assert_eq!(stats.pointer_moves(), 2);
        assert_eq!(stats.last_pointer(), Some((56, 89)));
    }

    #[test]
    fn pointer_positions_are_clamped_to_i32() {
        let mut stats = InputStats::default();
        stats.record_pointer_move(f64::MAX, f64::MIN);
        assert_eq!(stats.last_pointer(), Some((i32::MAX, i32::MIN)));
        stats.record_pointer_move(f64::NAN, 5.0);
        assert_eq!(stats.last_pointer(), Some((0, 5)));
    }

    #[test]
    fn button_transitions_are_counted() {
        let mut stats = InputStats::default();
        stats.record_pointer_button(true);
        stats.record_pointer_button(true);
        stats.record_pointer_button(false);
        assert_eq!(stats.pointer_button_presses(), 2);
        assert_eq!(stats.pointer_button_releases(), 1);
    }

    #[test]
    fn display_is_informative_and_panic_free() {
        let mut stats = InputStats::default();
        stats.record_key_press(false);
        stats.record_pointer_move(1.0, 2.0);
        let text = format!("{stats}");
        assert!(text.contains("1"), "summary: {text}");
    }
}
