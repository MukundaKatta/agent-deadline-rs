/*!
agent-deadline: cooperative per-task deadline for AI agent runs.

Agent loops chain LLM calls and tool calls. Each step has its own network
timeout, but rarely is there a single wall-clock cap on the whole task. This
crate is a small zero-dependency [`Deadline`] type that the loop checks between
steps and hands to downstream calls as their remaining timeout.

Cooperative model: code checks the deadline. Nothing is preempted.

```rust
use std::time::Duration;
use agent_deadline::{Deadline, DeadlineExceeded};

let deadline = Deadline::after(Duration::from_secs(30));

// Between steps, check whether the cap has passed.
deadline.check_or_err().unwrap();

// Plug the remaining time into per-call timeouts.
let remaining = deadline.remaining();
assert!(remaining <= Duration::from_secs(30));

// Tighten for a nested operation: pick the earlier of the two.
let retry = deadline.intersect(&Deadline::after(Duration::from_secs(5)));
assert!(retry.remaining() <= Duration::from_secs(5));
```
*/

use std::time::{Duration, Instant};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Returned by [`Deadline::check_or_err`] when a deadline has been exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlineExceeded {
    /// How long had elapsed since the deadline was created when it fired.
    pub elapsed: Duration,
}

impl DeadlineExceeded {
    /// The elapsed time as fractional seconds.
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }
}

impl std::fmt::Display for DeadlineExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "deadline exceeded after {:.3}s",
            self.elapsed.as_secs_f64()
        )
    }
}

impl std::error::Error for DeadlineExceeded {}

/// A cooperative per-task deadline.
///
/// A `Deadline` is a point in time (an [`Instant`]), or the sentinel
/// [`Deadline::never`] that never expires. Create one with [`Deadline::after`]
/// (relative to now), [`Deadline::at`] (an absolute instant), or
/// [`Deadline::never`], and call [`check_or_err`](Deadline::check_or_err) at
/// loop boundaries.
///
/// The deadline does NOT cancel work automatically — it returns
/// `Err(DeadlineExceeded)` so callers can stop voluntarily.
///
/// Equality compares the deadline instant only (two deadlines that fire at the
/// same instant are equal regardless of when they were created); two `never`
/// deadlines are equal, and `never` is never equal to a finite deadline.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    /// When this deadline was created. Used by [`elapsed`](Deadline::elapsed).
    created_at: Instant,
    /// The instant at which this deadline fires, or `None` for `never`.
    at: Option<Instant>,
}

impl Deadline {
    /// Create a deadline that fires `duration` from now.
    ///
    /// If adding `duration` to the current instant would overflow (for example
    /// [`Duration::MAX`]), the deadline falls back to [`never`](Deadline::never).
    pub fn after(duration: Duration) -> Self {
        let created_at = Instant::now();
        let at = created_at.checked_add(duration);
        Self { created_at, at }
    }

    /// Create a deadline that fires at the absolute instant `at`.
    pub fn at(at: Instant) -> Self {
        Self {
            created_at: Instant::now(),
            at: Some(at),
        }
    }

    /// Create a deadline that never fires.
    pub fn never() -> Self {
        Self {
            created_at: Instant::now(),
            at: None,
        }
    }

    /// True when this is the [`never`](Deadline::never) deadline.
    pub fn is_never(&self) -> bool {
        self.at.is_none()
    }

    /// The instant at which this deadline fires, or `None` for `never`.
    pub fn instant(&self) -> Option<Instant> {
        self.at
    }

    /// True once the current time has passed the deadline.
    ///
    /// Always `false` for [`never`](Deadline::never).
    pub fn expired(&self) -> bool {
        match self.at {
            Some(at) => Instant::now() >= at,
            None => false,
        }
    }

    /// Time remaining until the deadline, saturating at [`Duration::ZERO`].
    ///
    /// Returns [`Duration::MAX`] for [`never`](Deadline::never).
    pub fn remaining(&self) -> Duration {
        match self.at {
            Some(at) => at.saturating_duration_since(Instant::now()),
            None => Duration::MAX,
        }
    }

    /// Time remaining as fractional seconds.
    ///
    /// Returns [`f64::INFINITY`] for [`never`](Deadline::never).
    pub fn remaining_seconds(&self) -> f64 {
        match self.at {
            Some(_) => self.remaining().as_secs_f64(),
            None => f64::INFINITY,
        }
    }

    /// Time elapsed since this deadline was created.
    pub fn elapsed(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Return `Err(DeadlineExceeded)` if the deadline has been exceeded.
    ///
    /// A no-op (always `Ok`) for [`never`](Deadline::never).
    pub fn check_or_err(&self) -> Result<(), DeadlineExceeded> {
        if self.expired() {
            Err(DeadlineExceeded {
                elapsed: self.elapsed(),
            })
        } else {
            Ok(())
        }
    }

    /// Return a deadline that fires at the earlier of `self` and `other`.
    ///
    /// The returned deadline preserves `self`'s creation time, so
    /// [`elapsed`](Deadline::elapsed) keeps measuring from the original start.
    /// [`never`](Deadline::never) acts as the identity: intersecting with
    /// `never` never tightens the deadline.
    pub fn intersect(&self, other: &Deadline) -> Deadline {
        let at = match (self.at, other.at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        Deadline {
            created_at: self.created_at,
            at,
        }
    }

    /// Sugar for `self.intersect(&Deadline::after(duration))`.
    pub fn intersect_after(&self, duration: Duration) -> Deadline {
        self.intersect(&Deadline::after(duration))
    }
}

impl Default for Deadline {
    /// The default deadline is [`never`](Deadline::never).
    fn default() -> Self {
        Deadline::never()
    }
}

impl PartialEq for Deadline {
    /// Equality is by the deadline instant only; creation time is ignored.
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
    }
}

impl Eq for Deadline {}

#[cfg(feature = "serde")]
impl Serialize for Deadline {
    /// Serializes the remaining time as a snapshot: `None` for `never`,
    /// otherwise the remaining [`Duration`]. Deserializing reconstructs a
    /// deadline that fires roughly the same remaining time from the new "now".
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let snapshot: Option<Duration> = self.at.map(|_| self.remaining());
        snapshot.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Deadline {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let snapshot: Option<Duration> = Option::deserialize(deserializer)?;
        Ok(match snapshot {
            Some(remaining) => Deadline::after(remaining),
            None => Deadline::never(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn after_not_expired_when_in_future() {
        let d = Deadline::after(Duration::from_secs(3600));
        assert!(!d.expired());
        assert!(d.remaining() > Duration::ZERO);
        assert!(d.check_or_err().is_ok());
    }

    #[test]
    fn after_max_falls_back_to_never() {
        let d = Deadline::after(Duration::MAX);
        assert!(d.is_never());
    }

    #[test]
    fn never_basics() {
        let d = Deadline::never();
        assert!(d.is_never());
        assert!(!d.expired());
        assert_eq!(d.remaining(), Duration::MAX);
        assert!(d.remaining_seconds().is_infinite());
        assert!(d.check_or_err().is_ok());
        assert_eq!(d.instant(), None);
    }

    #[test]
    fn default_is_never() {
        assert!(Deadline::default().is_never());
    }

    #[test]
    fn expired_after_timeout() {
        let d = Deadline::after(Duration::from_millis(5));
        sleep(Duration::from_millis(20));
        assert!(d.expired());
        let err = d.check_or_err().unwrap_err();
        assert!(err.elapsed_seconds() > 0.0);
    }

    #[test]
    fn at_past_is_expired_and_remaining_zero() {
        let past = Instant::now() - Duration::from_secs(1);
        let d = Deadline::at(past);
        assert!(d.expired());
        assert_eq!(d.remaining(), Duration::ZERO);
    }

    #[test]
    fn intersect_picks_earlier() {
        let parent = Deadline::after(Duration::from_secs(3600));
        let child = parent.intersect(&Deadline::after(Duration::from_secs(1)));
        assert!(child.remaining() <= Duration::from_secs(1));
        assert!(child.instant() < parent.instant());
    }

    #[test]
    fn intersect_with_never_is_identity() {
        let parent = Deadline::after(Duration::from_secs(5));
        let child = parent.intersect(&Deadline::never());
        assert_eq!(child.instant(), parent.instant());
    }

    #[test]
    fn equality_by_instant() {
        let target = Instant::now() + Duration::from_secs(5);
        assert_eq!(Deadline::at(target), Deadline::at(target));
        assert_eq!(Deadline::never(), Deadline::never());
        assert_ne!(Deadline::never(), Deadline::after(Duration::from_secs(1)));
    }

    #[test]
    fn deadline_is_copy() {
        let d = Deadline::after(Duration::from_secs(5));
        let copied = d;
        assert_eq!(d.instant(), copied.instant());
    }

    #[test]
    fn deadline_exceeded_display_has_marker() {
        let err = DeadlineExceeded {
            elapsed: Duration::from_millis(1500),
        };
        assert!(err.to_string().contains("deadline exceeded"));
    }
}
