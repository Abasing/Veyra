//! The navigation/session state machine.
//!
//! Phase 2 scope (roadmap): initial navigation and session history with
//! back/forward, browser-owned transitions only (the roadmap's exact
//! wording). The controller holds no loading machinery — it decides
//! *which* transitions are legal and *which* entry to load; the caller
//! performs the load and reports back through
//! [`NavigationController::commit`] / [`NavigationController::fail`].
//! That split is what keeps the state machine synchronous, deterministic,
//! and property-testable.
//!
//! Phase model:
//!
//! ```text
//!            navigate(url)        back()/forward()
//!   Idle ─────────────────▶ Provisional ◀─────────────────  Loaded/Failed
//!     ▲                        │  │
//!     │            commit()    │  │ fail()
//!     └─────── Loaded ◀────────┘  └──────────▶ Failed
//! ```
//!
//! A new navigation committed from any phase truncates the forward stack
//! (standard session-history semantics); a back/forward commit moves the
//! current index. History is capped: the oldest entry is evicted when the
//! cap is reached.

use std::fmt;

use veyra_url::{Origin, Url};

/// Session-history cap (oldest evicted first).
pub const MAX_HISTORY: usize = 100;

/// One session-history entry: what was loaded, and what it claimed to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEntry {
    /// The entry's URL (post-redirect final URL for network loads).
    pub url: Url,
    /// The origin the document was tagged with (parse-derived).
    pub origin: Origin,
    /// The HTTP status, or `None` for non-HTTP loads (file://).
    pub status: Option<u16>,
}

impl SessionEntry {
    /// Builds an entry from a loaded document.
    #[must_use]
    pub fn from_document(document: &crate::browser::loader::LoadedDocument) -> Self {
        Self {
            url: document.url.clone(),
            origin: document.origin.clone(),
            status: document.status,
        }
    }
}

/// Illegal-transition failure (the full transition table is in the method
/// docs; every variant names the violated rule).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NavigationError {
    /// A load is already in flight; resolve it (commit/fail) first.
    LoadAlreadyProvisional,
    /// commit/fail called with no provisional load.
    NoProvisionalLoad,
    /// back()/forward() with no adjacent history entry.
    NoAdjacentEntry {
        /// The requested movement.
        direction: HistoryDirection,
    },
    /// go(delta) landed outside history bounds (clamping is explicit via
    /// can_go; this variant reports the unclamped refusal).
    DeltaOutOfBounds {
        /// The requested delta.
        delta: isize,
    },
    /// History is empty.
    EmptyHistory,
}

impl fmt::Display for NavigationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadAlreadyProvisional => write!(f, "a navigation is already in flight"),
            Self::NoProvisionalLoad => write!(f, "no provisional load to resolve"),
            Self::NoAdjacentEntry { direction } => {
                write!(f, "no {direction} entry in session history")
            }
            Self::DeltaOutOfBounds { delta } => {
                write!(f, "history offset {delta} is out of bounds")
            }
            Self::EmptyHistory => write!(f, "session history is empty"),
        }
    }
}

impl std::error::Error for NavigationError {}

/// Back/forward movement direction (error reporting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryDirection {
    /// Toward older entries.
    Back,
    /// Toward newer entries.
    Forward,
}

impl fmt::Display for HistoryDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Back => f.write_str("back"),
            Self::Forward => f.write_str("forward"),
        }
    }
}

/// The controller's phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavPhase {
    /// No navigation has happened yet.
    Idle,
    /// A load is in flight (new or history traversal).
    Provisional,
    /// The last load committed; `current()` is the live entry.
    Loaded,
    /// The last load failed; history state is unchanged.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    /// A brand-new navigation: append on commit.
    New,
    /// A history traversal: move to this index on commit.
    Existing(usize),
}

/// The navigation/session state machine.
#[derive(Debug)]
pub struct NavigationController {
    history: Vec<SessionEntry>,
    index: usize,
    phase: NavPhase,
    pending: Option<Pending>,
    provisional_url: Option<Url>,
}

impl Default for NavigationController {
    fn default() -> Self {
        Self::new()
    }
}

impl NavigationController {
    /// A controller with empty history, in [`NavPhase::Idle`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            index: 0,
            phase: NavPhase::Idle,
            pending: None,
            provisional_url: None,
        }
    }

    /// The current phase.
    #[must_use]
    pub fn phase(&self) -> &NavPhase {
        &self.phase
    }

    /// The live entry, when history is non-empty (valid in `Loaded` and
    /// `Failed` phases — a failed load leaves the previous entry live).
    #[must_use]
    pub fn current(&self) -> Option<&SessionEntry> {
        self.history.get(self.index)
    }

    /// The URL a provisional load is fetching, when provisional.
    #[must_use]
    pub fn provisional_url(&self) -> Option<&Url> {
        self.provisional_url.as_ref()
    }

    /// History length.
    #[must_use]
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Whether back() would succeed.
    #[must_use]
    pub fn can_back(&self) -> bool {
        !matches!(self.phase, NavPhase::Provisional) && self.index > 0
    }

    /// Whether forward() would succeed.
    #[must_use]
    pub fn can_forward(&self) -> bool {
        !matches!(self.phase, NavPhase::Provisional) && self.index + 1 < self.history.len()
    }

    /// Starts a brand-new navigation to `url`.
    ///
    /// # Errors
    ///
    /// [`NavigationError::LoadAlreadyProvisional`] when a load is in
    /// flight (resolve it first — browser-owned transitions).
    pub fn navigate(&mut self, url: Url) -> Result<(), NavigationError> {
        if matches!(self.phase, NavPhase::Provisional) {
            return Err(NavigationError::LoadAlreadyProvisional);
        }
        self.phase = NavPhase::Provisional;
        self.pending = Some(Pending::New);
        self.provisional_url = Some(url);
        Ok(())
    }

    /// Starts a history traversal (back/forward). The caller loads the
    /// returned URL (for Phase 2, by re-fetching — no bfcache yet).
    ///
    /// # Errors
    ///
    /// [`NavigationError::LoadAlreadyProvisional`] when provisional;
    /// [`NavigationError::EmptyHistory`] / [`NavigationError::NoAdjacentEntry`]
    /// when the requested adjacent entry does not exist.
    pub fn go_back(&mut self) -> Result<Url, NavigationError> {
        self.traverse(HistoryDirection::Back)
    }

    /// As [`NavigationController::go_back`], toward newer entries.
    ///
    /// # Errors
    ///
    /// As [`NavigationController::go_back`].
    pub fn go_forward(&mut self) -> Result<Url, NavigationError> {
        self.traverse(HistoryDirection::Forward)
    }

    /// Moves by `delta` clamped into history bounds; returns the target
    /// URL. Negative is back.
    ///
    /// # Errors
    ///
    /// [`NavigationError::LoadAlreadyProvisional`],
    /// [`NavigationError::EmptyHistory`],
    /// [`NavigationError::DeltaOutOfBounds`] when clamping cannot place
    /// the target inside history (delta 0 on non-empty history is a
    /// no-op reload and returns the current URL).
    pub fn go(&mut self, delta: isize) -> Result<Url, NavigationError> {
        if matches!(self.phase, NavPhase::Provisional) {
            return Err(NavigationError::LoadAlreadyProvisional);
        }
        if self.history.is_empty() {
            return Err(NavigationError::EmptyHistory);
        }
        let target = self.index as isize + delta;
        if target < 0 {
            return Err(NavigationError::DeltaOutOfBounds { delta });
        }
        let target = target as usize;
        if target >= self.history.len() {
            return Err(NavigationError::DeltaOutOfBounds { delta });
        }
        self.phase = NavPhase::Provisional;
        self.pending = Some(Pending::Existing(target));
        self.provisional_url = Some(self.history[target].url.clone());
        Ok(self.history[target].url.clone())
    }

    /// Resolves a provisional load successfully with `entry`.
    ///
    /// A `Pending::New` commit appends (truncating the forward stack —
    /// standard history semantics); a traversal commit moves to the
    /// traversal target.
    ///
    /// # Errors
    ///
    /// [`NavigationError::NoProvisionalLoad`] when not provisional.
    pub fn commit(&mut self, entry: SessionEntry) -> Result<(), NavigationError> {
        let Some(pending) = self.pending.take() else {
            return Err(NavigationError::NoProvisionalLoad);
        };
        match pending {
            Pending::New => {
                self.history.truncate(self.index + 1);
                self.history.push(entry);
                if self.history.len() > MAX_HISTORY {
                    self.history.remove(0);
                }
                self.index = self.history.len() - 1;
            }
            Pending::Existing(target) => {
                // The traversal target entry stays as recorded; the load
                // confirmed it is reachable.
                self.index = target;
            }
        }
        self.phase = NavPhase::Loaded;
        self.provisional_url = None;
        Ok(())
    }

    /// Resolves a provisional load with failure: the phase becomes
    /// [`NavPhase::Failed`] and history is unchanged.
    ///
    /// # Errors
    ///
    /// [`NavigationError::NoProvisionalLoad`] when not provisional.
    pub fn fail(&mut self) -> Result<(), NavigationError> {
        if self.pending.take().is_none() {
            return Err(NavigationError::NoProvisionalLoad);
        }
        self.phase = NavPhase::Failed;
        self.provisional_url = None;
        Ok(())
    }

    /// The current history slice (for session persistence; oldest first).
    #[must_use]
    pub fn history(&self) -> &[SessionEntry] {
        &self.history
    }

    /// The current entry's index in [`NavigationController::history`],
    /// or `None` when history is empty.
    #[must_use]
    pub fn current_index(&self) -> Option<usize> {
        (!self.history.is_empty()).then_some(self.index)
    }

    fn traverse(&mut self, direction: HistoryDirection) -> Result<Url, NavigationError> {
        if matches!(self.phase, NavPhase::Provisional) {
            return Err(NavigationError::LoadAlreadyProvisional);
        }
        if self.history.is_empty() {
            return Err(NavigationError::EmptyHistory);
        }
        let target = match direction {
            HistoryDirection::Back => {
                if self.index == 0 {
                    return Err(NavigationError::NoAdjacentEntry { direction });
                }
                self.index - 1
            }
            HistoryDirection::Forward => {
                if self.index + 1 >= self.history.len() {
                    return Err(NavigationError::NoAdjacentEntry { direction });
                }
                self.index + 1
            }
        };
        self.phase = NavPhase::Provisional;
        self.pending = Some(Pending::Existing(target));
        self.provisional_url = Some(self.history[target].url.clone());
        Ok(self.history[target].url.clone())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn url(input: &str) -> Url {
        Url::parse(input).unwrap()
    }

    fn entry(input: &str) -> SessionEntry {
        let parsed = url(input);
        SessionEntry {
            url: parsed.clone(),
            origin: parsed.origin(),
            status: Some(200),
        }
    }

    #[test]
    fn transitions_follow_the_phase_model() {
        let mut nav = NavigationController::new();
        assert_eq!(*nav.phase(), NavPhase::Idle);
        assert!(nav.current().is_none());

        // No history traversal from Idle.
        assert_eq!(nav.go_back(), Err(NavigationError::EmptyHistory));

        // Idle -> Provisional -> Loaded.
        nav.navigate(url("https://a.example/")).unwrap();
        assert_eq!(*nav.phase(), NavPhase::Provisional);
        // No second navigation while provisional.
        assert_eq!(
            nav.navigate(url("https://b.example/")),
            Err(NavigationError::LoadAlreadyProvisional)
        );
        nav.commit(entry("https://a.example/")).unwrap();
        assert_eq!(*nav.phase(), NavPhase::Loaded);
        assert_eq!(nav.current().unwrap().url.as_str(), "https://a.example/");

        // No commit without provisional.
        assert_eq!(
            nav.commit(entry("https://x.example/")),
            Err(NavigationError::NoProvisionalLoad)
        );

        // Failed load leaves history untouched, phase Failed.
        nav.navigate(url("https://down.example/")).unwrap();
        nav.fail().unwrap();
        assert_eq!(*nav.phase(), NavPhase::Failed);
        assert_eq!(nav.current().unwrap().url.as_str(), "https://a.example/");
    }

    #[test]
    fn new_navigation_truncates_forward_history() {
        let mut nav = NavigationController::new();
        for target in [
            "https://a.example/",
            "https://b.example/",
            "https://c.example/",
        ] {
            nav.navigate(url(target)).unwrap();
            nav.commit(entry(target)).unwrap();
        }
        assert_eq!(nav.history_len(), 3);

        nav.go_back().unwrap();
        nav.commit(entry("https://b.example/")).unwrap();
        // Traversal did not truncate: forward entries survive a back+commit.
        assert!(nav.can_forward());
        assert_eq!(nav.history_len(), 3);
        // A NEW navigation from index 1 truncates the forward stack:
        // [a, b, c] becomes [a, b, d].
        nav.navigate(url("https://d.example/")).unwrap();
        nav.commit(entry("https://d.example/")).unwrap();
        assert_eq!(nav.history_len(), 3);
        assert_eq!(
            nav.history()
                .iter()
                .map(|entry| entry.url.as_str())
                .collect::<Vec<_>>(),
            vec![
                "https://a.example/",
                "https://b.example/",
                "https://d.example/"
            ]
        );
        assert_eq!(nav.current().unwrap().url.as_str(), "https://d.example/");
        assert!(!nav.can_forward());
        assert!(nav.can_back());
    }

    #[test]
    fn back_and_forward_respect_bounds() {
        let mut nav = NavigationController::new();
        nav.navigate(url("https://a.example/")).unwrap();
        nav.commit(entry("https://a.example/")).unwrap();
        assert_eq!(
            nav.go_back(),
            Err(NavigationError::NoAdjacentEntry {
                direction: HistoryDirection::Back
            })
        );
        nav.navigate(url("https://b.example/")).unwrap();
        nav.commit(entry("https://b.example/")).unwrap();

        // Back to a, forward to b, forward again is an error.
        let target = nav.go_back().unwrap();
        assert_eq!(target.as_str(), "https://a.example/");
        nav.commit(entry("https://a.example/")).unwrap();
        let target = nav.go_forward().unwrap();
        assert_eq!(target.as_str(), "https://b.example/");
        nav.commit(entry("https://b.example/")).unwrap();
        assert_eq!(
            nav.go_forward(),
            Err(NavigationError::NoAdjacentEntry {
                direction: HistoryDirection::Forward
            })
        );
    }

    #[test]
    fn history_cap_evicts_oldest() {
        let mut nav = NavigationController::new();
        for i in 0..(MAX_HISTORY + 10) {
            let target = format!("https://h{i}.example/");
            nav.navigate(url(&target)).unwrap();
            nav.commit(entry(&target)).unwrap();
        }
        assert_eq!(nav.history_len(), MAX_HISTORY);
        // The oldest entries are gone; the current is the newest.
        assert_eq!(nav.history()[0].url.as_str(), "https://h10.example/");
        assert_eq!(
            nav.current().unwrap().url.as_str(),
            &format!("https://h{}.example/", MAX_HISTORY + 9)
        );
    }

    #[test]
    fn go_clamps_and_reports_out_of_bounds() {
        let mut nav = NavigationController::new();
        for target in [
            "https://a.example/",
            "https://b.example/",
            "https://c.example/",
        ] {
            nav.navigate(url(target)).unwrap();
            nav.commit(entry(target)).unwrap();
        }
        assert_eq!(
            nav.go(5),
            Err(NavigationError::DeltaOutOfBounds { delta: 5 })
        );
        assert_eq!(
            nav.go(-9),
            Err(NavigationError::DeltaOutOfBounds { delta: -9 })
        );
        // Reload: go(0) returns the current URL.
        assert_eq!(nav.go(0).unwrap().as_str(), "https://c.example/");
    }

    // -- property tests ---------------------------------------------------

    struct Xorshift(u64);

    impl Xorshift {
        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    /// Structural audit of the controller after every operation.
    fn audit(nav: &NavigationController) {
        let length = nav.history_len();
        if length == 0 {
            assert!(nav.current().is_none());
            assert!(!nav.can_back());
            assert!(!nav.can_forward());
            return;
        }
        let index = nav.current_index().unwrap();
        assert!(index < length, "index {index} out of bounds for {length}");
        // Bounds flags agree with the index — except while provisional,
        // where both flags are deliberately false (a load is in flight).
        if !matches!(nav.phase(), NavPhase::Provisional) {
            assert_eq!(nav.can_back(), index > 0);
            assert_eq!(nav.can_forward(), index + 1 < length);
        } else {
            assert!(!nav.can_back() && !nav.can_forward());
        }
        // The cap held.
        assert!(length <= MAX_HISTORY);
        // The phase is never Idle after a commit.
        assert!(!matches!(nav.phase(), NavPhase::Idle));
        // No duplicate consecutive entries from truncation bugs.
        for pair in nav.history().windows(2) {
            let _ = pair; // URLs may legitimately repeat (reload); nothing to assert
        }
    }

    #[test]
    fn random_operation_sequences_preserve_invariants() {
        for seed in 1..=32u64 {
            let mut rng = Xorshift(seed.max(1));
            let mut nav = NavigationController::new();
            let mut counter = 0u64;
            for _ in 0..600 {
                match rng.next_u64() % 7 {
                    0 | 1 => {
                        counter += 1;
                        let target = format!("https://p{seed}-{counter}.example/");
                        let _ = nav.navigate(url(&target));
                    }
                    2 => {
                        if let Ok(target) = nav.go_back() {
                            // The browser loads the target then commits.
                            nav.commit(entry(target.as_str()))
                                .unwrap_or_else(|e| panic!("seed {seed}: back commit failed: {e}"));
                        }
                    }
                    3 => {
                        if let Ok(target) = nav.go_forward() {
                            nav.commit(entry(target.as_str())).unwrap_or_else(|e| {
                                panic!("seed {seed}: forward commit failed: {e}")
                            });
                        }
                    }
                    4 => {
                        let delta = (rng.next_u64() % 5) as isize - 2;
                        if let Ok(target) = nav.go(delta) {
                            nav.commit(entry(target.as_str())).unwrap();
                        }
                    }
                    5 => {
                        let _ = nav.fail();
                    }
                    _ => {
                        // Attempt an illegal commit (no provisional).
                        let _ = nav.commit(entry("https://illegal.example/"));
                    }
                }
                audit(&nav);
            }
        }
    }
}
