/*!
agent-deadline: cooperative per-task deadline for AI agent runs.

Check the deadline at loop boundaries. Raise (return Err) when time is up.
Supports intersecting two deadlines to get the stricter one.

```rust
use agent_deadline::Deadline;

let d = Deadline::new(3600.0, Some("my_task".to_string()));
assert!(!d.is_exceeded());
assert!(d.remaining_secs() > 0.0);
d.check_or_raise().unwrap();
```
*/

use std::time::{Duration, Instant};

/// Raised when a deadline has been exceeded.
#[derive(Debug, Clone, PartialEq)]
pub struct DeadlineExceeded {
    pub elapsed_secs: f64,
    pub allowed_secs: f64,
    pub task_id: Option<String>,
}

impl std::fmt::Display for DeadlineExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DeadlineExceeded: elapsed {:.3}s > allowed {:.3}s{}",
            self.elapsed_secs,
            self.allowed_secs,
            self.task_id
                .as_deref()
                .map(|id| format!(" (task={})", id))
                .unwrap_or_default()
        )
    }
}

impl std::error::Error for DeadlineExceeded {}

/// A cooperative per-task deadline.
///
/// Create with `Deadline::new(seconds, task_id)` and call `check_or_raise()`
/// at loop boundaries. The deadline does NOT cancel work automatically — it
/// returns `Err(DeadlineExceeded)` so callers can stop voluntarily.
#[derive(Clone)]
pub struct Deadline {
    start: Instant,
    duration: Duration,
    pub task_id: Option<String>,
}

impl Deadline {
    /// Create a new deadline that expires `seconds` from now.
    pub fn new(seconds: f64, task_id: Option<String>) -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_secs_f64(seconds.max(0.0)),
            task_id,
        }
    }

    /// Seconds elapsed since this deadline was created.
    pub fn elapsed_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Seconds remaining before the deadline expires. Returns 0.0 when exceeded.
    pub fn remaining_secs(&self) -> f64 {
        let elapsed = self.start.elapsed();
        if elapsed >= self.duration {
            0.0
        } else {
            (self.duration - elapsed).as_secs_f64()
        }
    }

    /// True when the deadline has been exceeded.
    pub fn is_exceeded(&self) -> bool {
        self.start.elapsed() >= self.duration
    }

    /// Return `Err(DeadlineExceeded)` if the deadline has been exceeded.
    pub fn check_or_raise(&self) -> Result<(), DeadlineExceeded> {
        let elapsed = self.start.elapsed().as_secs_f64();
        let allowed = self.duration.as_secs_f64();
        if elapsed >= allowed {
            Err(DeadlineExceeded {
                elapsed_secs: elapsed,
                allowed_secs: allowed,
                task_id: self.task_id.clone(),
            })
        } else {
            Ok(())
        }
    }

    /// Return a new deadline whose expiry is the sooner of `self` and `other`.
    pub fn intersect(&self, other: &Deadline) -> Deadline {
        let self_remaining = self.remaining_secs();
        let other_remaining = other.remaining_secs();
        let task_id = if self_remaining <= other_remaining {
            self.task_id.clone()
        } else {
            other.task_id.clone()
        };
        Deadline::new(self_remaining.min(other_remaining), task_id)
    }

    /// The total allowed duration in seconds.
    pub fn allowed_secs(&self) -> f64 {
        self.duration.as_secs_f64()
    }
}

impl std::fmt::Debug for Deadline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deadline")
            .field("elapsed_secs", &self.elapsed_secs())
            .field("remaining_secs", &self.remaining_secs())
            .field("task_id", &self.task_id)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn new_deadline_not_exceeded() {
        let d = Deadline::new(3600.0, None);
        assert!(!d.is_exceeded());
    }

    #[test]
    fn check_or_raise_ok_when_not_exceeded() {
        let d = Deadline::new(3600.0, None);
        assert!(d.check_or_raise().is_ok());
    }

    #[test]
    fn elapsed_increases_over_time() {
        let d = Deadline::new(3600.0, None);
        thread::sleep(Duration::from_millis(20));
        assert!(d.elapsed_secs() > 0.0);
    }

    #[test]
    fn remaining_decreases_over_time() {
        let d = Deadline::new(3600.0, None);
        let r1 = d.remaining_secs();
        thread::sleep(Duration::from_millis(20));
        let r2 = d.remaining_secs();
        assert!(r1 > r2);
    }

    #[test]
    fn exceeded_after_timeout() {
        let d = Deadline::new(0.01, None);
        thread::sleep(Duration::from_millis(30));
        assert!(d.is_exceeded());
    }

    #[test]
    fn check_or_raise_err_when_exceeded() {
        let d = Deadline::new(0.01, Some("test_task".to_string()));
        thread::sleep(Duration::from_millis(30));
        let err = d.check_or_raise().unwrap_err();
        assert_eq!(err.task_id, Some("test_task".to_string()));
        assert!(err.elapsed_secs > 0.0);
        assert!(err.allowed_secs < 0.1);
    }

    #[test]
    fn remaining_zero_when_exceeded() {
        let d = Deadline::new(0.01, None);
        thread::sleep(Duration::from_millis(30));
        assert_eq!(d.remaining_secs(), 0.0);
    }

    #[test]
    fn task_id_stored() {
        let d = Deadline::new(3600.0, Some("my_task".to_string()));
        assert_eq!(d.task_id, Some("my_task".to_string()));
    }

    #[test]
    fn no_task_id() {
        let d = Deadline::new(3600.0, None);
        assert_eq!(d.task_id, None);
    }

    #[test]
    fn zero_second_deadline_immediately_exceeded() {
        let d = Deadline::new(0.0, None);
        thread::sleep(Duration::from_millis(5));
        assert!(d.is_exceeded());
    }

    #[test]
    fn negative_seconds_treated_as_zero() {
        let d = Deadline::new(-1.0, None);
        thread::sleep(Duration::from_millis(5));
        assert!(d.is_exceeded());
    }

    #[test]
    fn intersect_picks_shorter() {
        let short = Deadline::new(10.0, Some("short".to_string()));
        let long = Deadline::new(3600.0, Some("long".to_string()));
        let intersected = short.intersect(&long);
        assert!(intersected.remaining_secs() <= 10.1);
        assert_eq!(intersected.task_id, Some("short".to_string()));
    }

    #[test]
    fn intersect_picks_shorter_reversed() {
        let short = Deadline::new(10.0, Some("short".to_string()));
        let long = Deadline::new(3600.0, Some("long".to_string()));
        let intersected = long.intersect(&short);
        assert!(intersected.remaining_secs() <= 10.1);
        assert_eq!(intersected.task_id, Some("short".to_string()));
    }

    #[test]
    fn intersect_both_exceeded() {
        let a = Deadline::new(0.01, Some("a".to_string()));
        let b = Deadline::new(0.01, Some("b".to_string()));
        thread::sleep(Duration::from_millis(30));
        let intersected = a.intersect(&b);
        assert_eq!(intersected.remaining_secs(), 0.0);
    }

    #[test]
    fn deadline_exceeded_display() {
        let err = DeadlineExceeded {
            elapsed_secs: 1.5,
            allowed_secs: 1.0,
            task_id: Some("task1".to_string()),
        };
        let s = err.to_string();
        assert!(s.contains("elapsed 1.500s"));
        assert!(s.contains("allowed 1.000s"));
        assert!(s.contains("task1"));
    }

    #[test]
    fn deadline_exceeded_no_task_id_display() {
        let err = DeadlineExceeded {
            elapsed_secs: 1.0,
            allowed_secs: 0.5,
            task_id: None,
        };
        let s = err.to_string();
        assert!(!s.contains("task="));
    }

    #[test]
    fn allowed_secs_matches_input() {
        let d = Deadline::new(42.0, None);
        assert!((d.allowed_secs() - 42.0).abs() < 0.001);
    }

    #[test]
    fn clone_works() {
        let d = Deadline::new(3600.0, Some("t".to_string()));
        let d2 = d.clone();
        assert_eq!(d2.task_id, d.task_id);
    }
}
