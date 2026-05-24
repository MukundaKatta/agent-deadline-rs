//! Cooperative per-task deadline primitive for AI agent workflows.
//!
//! Agent loops chain LLM and tool calls. Each step has its own network
//! timeout, but rarely is there a single wall-clock cap on the whole task.
//! This crate is a small zero-dependency [`Deadline`] type that the loop
//! checks between steps and hands to downstream calls as their remaining
//! timeout.
//!
//! Cooperative model: code checks the deadline. Nothing is preempted.
//!
//! # Example
//!
//! ```
//! use std::time::Duration;
//! use agent_deadline::Deadline;
//!
//! let deadline = Deadline::after(Duration::from_secs(30));
//!
//! // Between agent steps, check whether the cap has passed.
//! deadline.check_or_err().unwrap();
//!
//! // Plug the remaining time into per-call timeouts.
//! let remaining = deadline.remaining();
//! assert!(remaining <= Duration::from_secs(30));
//! ```
//!
//! # Intersecting nested deadlines
//!
//! A sub-step often has its own soft cap that should never exceed the
//! parent task's remaining time. [`Deadline::intersect`] picks the earlier
//! of the two.
//!
//! ```
//! use std::time::Duration;
//! use agent_deadline::Deadline;
//!
//! let task = Deadline::after(Duration::from_secs(30));
//! let retry = Deadline::after(Duration::from_secs(5));
//!
//! let tighter = task.intersect(&retry);
//! assert!(tighter.remaining() <= Duration::from_secs(5));
//! ```

use std::error::Error;
use std::fmt;
use std::time::{Duration, Instant};

/// Internal representation of when a deadline fires.
///
/// `Instant` is used because it is monotonic on every supported platform,
/// so the deadline is unaffected by NTP jumps or wall-clock resets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeadlineAt {
    At(Instant),
    Never,
}

/// A monotonic-time deadline for cooperative task cancellation.
///
/// Build one with [`Deadline::after`] (relative) or [`Deadline::at`]
/// (absolute), then call [`Deadline::check_or_err`] between agent steps
/// and [`Deadline::remaining`] when handing the timeout to a per-call API.
///
/// Deadlines are immutable. To tighten a deadline for a nested operation
/// without touching the original, use [`Deadline::intersect`].
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    at: DeadlineAt,
    /// Captured at build time so `elapsed` keeps measuring against the
    /// original task start, even after an intersection.
    created_at: Instant,
}

impl Deadline {
    /// Build a deadline that fires `duration` from now.
    ///
    /// A `Duration::ZERO` produces an already-expired deadline, which is
    /// occasionally useful in tests.
    pub fn after(duration: Duration) -> Self {
        let now = Instant::now();
        // `checked_add` returns None on overflow with very large durations.
        // Treat that as "never" so callers asking for a near-infinite cap
        // do not get a silently truncated deadline.
        let at = match now.checked_add(duration) {
            Some(t) => DeadlineAt::At(t),
            None => DeadlineAt::Never,
        };
        Self { at, created_at: now }
    }

    /// Build a deadline that fires at an exact monotonic instant.
    ///
    /// Use this when you already have an `Instant` from another source
    /// (for example a parent task's deadline expressed as an `Instant`).
    pub fn at(instant: Instant) -> Self {
        Self {
            at: DeadlineAt::At(instant),
            created_at: Instant::now(),
        }
    }

    /// A deadline that never fires.
    ///
    /// Useful as a default for callers that may or may not want a cap.
    /// [`Self::remaining`] returns `Duration::MAX`, [`Self::expired`] is
    /// always `false`, and [`Self::check_or_err`] is a no-op.
    pub fn never() -> Self {
        Self {
            at: DeadlineAt::Never,
            created_at: Instant::now(),
        }
    }

    /// `true` if this deadline was built with [`Deadline::never`].
    pub fn is_never(&self) -> bool {
        matches!(self.at, DeadlineAt::Never)
    }

    /// The absolute instant at which this deadline fires, if any.
    ///
    /// Returns `None` for [`Deadline::never`].
    pub fn instant(&self) -> Option<Instant> {
        match self.at {
            DeadlineAt::At(i) => Some(i),
            DeadlineAt::Never => None,
        }
    }

    /// `true` once the current monotonic time is at or past the deadline.
    ///
    /// Always `false` for [`Deadline::never`].
    pub fn expired(&self) -> bool {
        match self.at {
            DeadlineAt::Never => false,
            DeadlineAt::At(at) => Instant::now() >= at,
        }
    }

    /// Duration left until the deadline. Saturates at zero. Never negative.
    ///
    /// Returns [`Duration::MAX`] for [`Deadline::never`]. Hand this to any
    /// timeout-taking API.
    pub fn remaining(&self) -> Duration {
        match self.at {
            DeadlineAt::Never => Duration::MAX,
            DeadlineAt::At(at) => at.saturating_duration_since(Instant::now()),
        }
    }

    /// Convenience for callers that want the remaining time as `f64` seconds.
    ///
    /// Returns `f64::INFINITY` for [`Deadline::never`].
    pub fn remaining_seconds(&self) -> f64 {
        match self.at {
            DeadlineAt::Never => f64::INFINITY,
            DeadlineAt::At(_) => self.remaining().as_secs_f64(),
        }
    }

    /// Seconds since this deadline was constructed.
    ///
    /// Survives an [`Self::intersect`]: the elapsed counter still measures
    /// against the original task start.
    pub fn elapsed(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Cooperative check: return `Err(DeadlineExceeded)` if the deadline
    /// has passed.
    ///
    /// No-op for [`Deadline::never`]. Call between agent steps.
    pub fn check_or_err(&self) -> Result<(), DeadlineExceeded> {
        match self.at {
            DeadlineAt::Never => Ok(()),
            DeadlineAt::At(at) => {
                let now = Instant::now();
                if now >= at {
                    Err(DeadlineExceeded {
                        elapsed: now.saturating_duration_since(self.created_at),
                    })
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Return a new deadline that fires at the earlier of `self` and `other`.
    ///
    /// `created_at` is preserved from `self` so [`Self::elapsed`] keeps
    /// measuring against the original task start, not the intersection point.
    pub fn intersect(&self, other: &Deadline) -> Deadline {
        let tighter = match (self.at, other.at) {
            (DeadlineAt::Never, other_at) => other_at,
            (self_at, DeadlineAt::Never) => self_at,
            (DeadlineAt::At(a), DeadlineAt::At(b)) => DeadlineAt::At(a.min(b)),
        };
        Deadline {
            at: tighter,
            created_at: self.created_at,
        }
    }

    /// Convenience: intersect with a deadline `duration` from now.
    pub fn intersect_after(&self, duration: Duration) -> Deadline {
        self.intersect(&Deadline::after(duration))
    }
}

impl Default for Deadline {
    /// Default deadline is [`Deadline::never`]. Useful in struct fields
    /// where "no cap" is the sane default.
    fn default() -> Self {
        Self::never()
    }
}

impl PartialEq for Deadline {
    /// Two deadlines compare equal when they fire at the same instant
    /// (or both are `never`). `created_at` is not part of identity.
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
    }
}

impl Eq for Deadline {}

/// Error returned by [`Deadline::check_or_err`] when the cap has passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlineExceeded {
    /// Time since the deadline was created.
    pub elapsed: Duration,
}

impl DeadlineExceeded {
    /// Elapsed time since the deadline was created, as `f64` seconds.
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }
}

impl fmt::Display for DeadlineExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "deadline exceeded: elapsed {:.6}s past the configured cap",
            self.elapsed_seconds()
        )
    }
}

impl Error for DeadlineExceeded {}

// ---- optional serde support ----
//
// `Instant` is not portably serializable: it has no fixed reference point
// across processes. For diagnostic snapshots we serialize the *remaining*
// duration at the moment of serialization, plus a `never` flag.
// Round-tripping reconstructs a deadline that fires the same duration from
// the deserializing process's `now`. This is a snapshot, not a faithful
// clone, and that is the documented intent.

#[cfg(feature = "serde")]
mod serde_impl {
    use super::{Deadline, DeadlineAt};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    struct DeadlineSnapshot {
        never: bool,
        remaining_secs: f64,
    }

    impl Serialize for Deadline {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            // JSON has no `Infinity`, so for the `never` case we just emit
            // a sentinel `remaining_secs = 0.0` and rely on the `never` flag.
            let snap = match self.at {
                DeadlineAt::Never => DeadlineSnapshot {
                    never: true,
                    remaining_secs: 0.0,
                },
                DeadlineAt::At(_) => DeadlineSnapshot {
                    never: false,
                    remaining_secs: self.remaining().as_secs_f64(),
                },
            };
            snap.serialize(ser)
        }
    }

    impl<'de> Deserialize<'de> for Deadline {
        fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
            let snap = DeadlineSnapshot::deserialize(de)?;
            if snap.never {
                return Ok(Deadline::never());
            }
            let remaining = if snap.remaining_secs.is_finite() && snap.remaining_secs > 0.0 {
                Duration::from_secs_f64(snap.remaining_secs)
            } else {
                Duration::ZERO
            };
            Ok(Deadline::after(remaining))
        }
    }
}
