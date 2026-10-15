//! Injectable frame clock (Phase 1 substrate for the Phase 4 executor).
//!
//! Design rules (roadmap Phase 1, engineering standards §8):
//!
//! - Tests advance time **explicitly** — no wall-clock sleeps, ever.
//! - Frame progression is reproducible: the same call sequence on the same
//!   clock produces the same timestamps.
//! - The abstraction does **not** depend on a live display refresh rate.
//!   There is no vsync/refresh coupling here; production timing and test
//!   timing are two implementations behind one tiny interface.
//! - This is *not* the HTML event loop: no timers, no rAF callbacks, no
//!   scheduling semantics. Those are Phase 4 (`veyra-executor`).
//!
//! The nominal frame interval constant exists only so later phases and
//! the benchmark report can state "60 Hz-equivalent" precisely; nothing
//! in Phase 1 paces against it.

use std::fmt;
use std::time::Instant;

use crate::error::ClockError;

/// A monotonic source of logical frame time.
///
/// `frame_time_micros` returns the clock's current logical time;
/// `advance_frame` moves the clock to the next frame boundary. Production
/// code uses [`SystemFrameClock`] (real elapsed time); tests use
/// [`FixedFrameClock`] (explicit stepping).
pub trait FrameClock: fmt::Debug {
    /// Logical microseconds since the clock's epoch. Monotonically
    /// non-decreasing.
    fn frame_time_micros(&self) -> u64;

    /// Advances the clock by one frame. For the system clock this is a
    /// no-op (real time flows on its own); for the fixed clock it adds the
    /// configured step.
    ///
    /// # Errors
    /// [`ClockError::Overflow`] when the fixed clock's counter would
    /// overflow (a test running for ~585 millennia at microsecond
    /// resolution).
    fn advance_frame(&mut self) -> Result<(), ClockError>;
}

/// Production clock: real elapsed time since construction.
#[derive(Debug)]
pub struct SystemFrameClock {
    start: Instant,
}

impl SystemFrameClock {
    /// Starts a clock whose epoch is "now".
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemFrameClock {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameClock for SystemFrameClock {
    fn frame_time_micros(&self) -> u64 {
        self.start.elapsed().as_micros().min(u64::MAX as u128) as u64
    }

    fn advance_frame(&mut self) -> Result<(), ClockError> {
        // Real time advances on its own; nothing to do.
        Ok(())
    }
}

/// Test clock: deterministic fixed-step time, advanced explicitly.
///
/// The step is in microseconds and must be >= 1 (a zero step would freeze
/// logical time, silently breaking any assertion on progression — it is
/// rejected at construction instead).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedFrameClock {
    step_micros: u64,
    current_micros: u64,
}

impl FixedFrameClock {
    /// Creates a fixed-step clock starting at logical time 0.
    ///
    /// # Errors
    /// [`ClockError::ZeroStep`] when `step_micros == 0`.
    pub fn new(step_micros: u64) -> Result<Self, ClockError> {
        if step_micros == 0 {
            return Err(ClockError::ZeroStep);
        }
        Ok(Self {
            step_micros,
            current_micros: 0,
        })
    }

    /// The configured step in microseconds.
    pub fn step_micros(&self) -> u64 {
        self.step_micros
    }

    /// The current logical time without advancing.
    pub fn current_micros(&self) -> u64 {
        self.current_micros
    }
}

impl FrameClock for FixedFrameClock {
    fn frame_time_micros(&self) -> u64 {
        self.current_micros
    }

    fn advance_frame(&mut self) -> Result<(), ClockError> {
        self.current_micros = self
            .current_micros
            .checked_add(self.step_micros)
            .ok_or(ClockError::Overflow)?;
        Ok(())
    }
}

/// Nominal 60 Hz frame interval in microseconds, stated for documentation
/// and later-phase budgeting. Phase 1 does not pace against a display.
pub const NOMINAL_FRAME_INTERVAL_MICROS: u64 = 16_666;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn fixed_clock_progresses_deterministically() {
        let mut clock = FixedFrameClock::new(NOMINAL_FRAME_INTERVAL_MICROS).expect("valid step");
        let mut timestamps = Vec::new();
        for _ in 0..5 {
            clock.advance_frame().expect("no overflow");
            timestamps.push(clock.frame_time_micros());
        }
        assert_eq!(
            timestamps,
            vec![16_666, 33_332, 49_998, 66_664, 83_330],
            "progression must be exactly reproducible"
        );
        assert_eq!(clock.current_micros(), 83_330);
        assert_eq!(clock.step_micros(), NOMINAL_FRAME_INTERVAL_MICROS);
    }

    #[test]
    fn fixed_clock_starts_at_zero() {
        let clock = FixedFrameClock::new(1_000).expect("valid step");
        assert_eq!(clock.frame_time_micros(), 0);
    }

    #[test]
    fn zero_step_is_rejected() {
        assert_eq!(FixedFrameClock::new(0), Err(ClockError::ZeroStep));
    }

    #[test]
    fn fixed_clock_reports_overflow_instead_of_wrapping() {
        let mut clock = FixedFrameClock::new(u64::MAX).expect("valid step");
        // First advance: 0 + u64::MAX = u64::MAX — succeeds.
        assert_eq!(clock.advance_frame(), Ok(()));
        assert_eq!(clock.frame_time_micros(), u64::MAX);
        // Second advance would exceed the counter: reported, not wrapped.
        assert_eq!(clock.advance_frame(), Err(ClockError::Overflow));
        // The failed advance left the clock unchanged.
        assert_eq!(clock.frame_time_micros(), u64::MAX);
    }

    #[test]
    fn system_clock_is_monotonically_nondecreasing() {
        let clock = SystemFrameClock::new();
        let mut previous = clock.frame_time_micros();
        for _ in 0..100 {
            let current = clock.frame_time_micros();
            assert!(current >= previous, "system clock went backwards");
            previous = current;
        }
    }

    #[test]
    fn system_clock_advance_is_a_noop_that_succeeds() {
        let mut clock = SystemFrameClock::new();
        assert_eq!(clock.advance_frame(), Ok(()));
    }

    #[test]
    fn clocks_are_debuggable() {
        // Part of the trait contract: diagnostics can print clocks.
        assert!(!format!("{:?}", FixedFrameClock::new(5).expect("valid")).is_empty());
        assert!(!format!("{:?}", SystemFrameClock::new()).is_empty());
    }
}
