//! Integration tests for `agent-deadline`.
//!
//! Mirror of the Python reference test suite where it makes sense, plus
//! Rust-specific cases (overflow on `after`, Default impl, equality).

use agent_deadline::{Deadline, DeadlineExceeded};
use std::thread::sleep;
use std::time::{Duration, Instant};

// ---------- factories ----------

#[test]
fn after_sets_instant_relative_to_monotonic() {
    let before = Instant::now();
    let d = Deadline::after(Duration::from_millis(500));
    let after = Instant::now();
    let at = d.instant().expect("finite deadline has an instant");
    assert!(
        at >= before + Duration::from_millis(500),
        "deadline must not fire earlier than `before + duration`"
    );
    assert!(
        at <= after + Duration::from_millis(500) + Duration::from_micros(1),
        "deadline must not fire later than `after + duration` (slack 1us)"
    );
}

#[test]
fn after_zero_gives_already_expired_deadline() {
    let d = Deadline::after(Duration::ZERO);
    assert!(d.expired());
    assert_eq!(d.remaining(), Duration::ZERO);
}

#[test]
fn after_with_max_duration_falls_back_to_never() {
    // Duration::MAX overflows when added to any Instant.
    let d = Deadline::after(Duration::MAX);
    assert!(d.is_never(), "overflow should fall back to never");
}

#[test]
fn never_is_not_expired_and_remaining_is_max() {
    let d = Deadline::never();
    assert!(d.is_never());
    assert!(!d.expired());
    assert_eq!(d.remaining(), Duration::MAX);
    assert!(d.remaining_seconds().is_infinite());
}

#[test]
fn never_check_or_err_is_noop() {
    let d = Deadline::never();
    for _ in 0..5 {
        d.check_or_err().expect("never should never error");
    }
}

#[test]
fn at_with_future_instant_constructs_finite_deadline() {
    let target = Instant::now() + Duration::from_secs(1);
    let d = Deadline::at(target);
    assert_eq!(d.instant(), Some(target));
    assert!(!d.expired());
}

#[test]
fn default_is_never() {
    let d: Deadline = Deadline::default();
    assert!(d.is_never());
}

// ---------- expired ----------

#[test]
fn expired_false_when_in_future() {
    let d = Deadline::after(Duration::from_secs(10));
    assert!(!d.expired());
}

#[test]
fn expired_true_when_past() {
    let d = Deadline::after(Duration::from_millis(1));
    sleep(Duration::from_millis(15));
    assert!(d.expired());
}

// ---------- remaining ----------

#[test]
fn remaining_positive_when_in_future() {
    let d = Deadline::after(Duration::from_secs(1));
    let r = d.remaining();
    assert!(r > Duration::ZERO);
    assert!(r <= Duration::from_secs(1));
}

#[test]
fn remaining_zero_when_expired() {
    let d = Deadline::after(Duration::from_millis(1));
    sleep(Duration::from_millis(15));
    assert_eq!(d.remaining(), Duration::ZERO);
}

#[test]
fn remaining_saturates_at_zero_not_negative() {
    // `Duration` cannot be negative, but the contract is that we saturate
    // rather than panic on an `at` in the past.
    let past = Instant::now() - Duration::from_secs(1);
    let d = Deadline::at(past);
    assert_eq!(d.remaining(), Duration::ZERO);
}

#[test]
fn remaining_seconds_finite_for_finite_deadline() {
    let d = Deadline::after(Duration::from_secs(2));
    let r = d.remaining_seconds();
    assert!(r.is_finite());
    assert!(r > 0.0);
    assert!(r <= 2.0);
}

// ---------- check_or_err ----------

#[test]
fn check_or_err_ok_when_in_future() {
    let d = Deadline::after(Duration::from_secs(5));
    d.check_or_err().expect("should not error");
}

#[test]
fn check_or_err_errs_when_past() {
    let d = Deadline::after(Duration::from_millis(1));
    sleep(Duration::from_millis(15));
    let err = d.check_or_err().expect_err("should have errored");
    assert!(err.elapsed > Duration::ZERO);
}

#[test]
fn deadline_exceeded_display_includes_marker() {
    let d = Deadline::after(Duration::from_millis(1));
    sleep(Duration::from_millis(20));
    let err = d.check_or_err().expect_err("should have errored");
    let msg = format!("{}", err);
    assert!(msg.contains("deadline exceeded"), "got: {msg}");
}

#[test]
fn deadline_exceeded_carries_elapsed_seconds() {
    let d = Deadline::after(Duration::from_millis(1));
    sleep(Duration::from_millis(25));
    let err: DeadlineExceeded = d.check_or_err().expect_err("should have errored");
    assert!(err.elapsed_seconds() >= 0.020);
}

#[test]
fn deadline_exceeded_is_error_trait() {
    fn assert_error<E: std::error::Error>(_: &E) {}
    let d = Deadline::after(Duration::ZERO);
    if let Err(e) = d.check_or_err() {
        assert_error(&e);
    } else {
        panic!("Duration::ZERO should expire immediately");
    }
}

// ---------- elapsed ----------

#[test]
fn elapsed_counts_up() {
    let d = Deadline::after(Duration::from_secs(10));
    let e1 = d.elapsed();
    sleep(Duration::from_millis(20));
    let e2 = d.elapsed();
    assert!(e2 > e1);
    assert!(e2 - e1 >= Duration::from_millis(15));
}

#[test]
fn elapsed_starts_near_zero() {
    let d = Deadline::after(Duration::from_secs(10));
    assert!(d.elapsed() < Duration::from_millis(50));
}

// ---------- intersect ----------

#[test]
fn intersect_picks_tighter_when_other_sooner() {
    let parent = Deadline::after(Duration::from_secs(60));
    let child_target = Deadline::after(Duration::from_secs(1));
    let child = parent.intersect(&child_target);
    assert!(child.remaining() <= Duration::from_secs(1));
    assert!(child.instant() < parent.instant());
}

#[test]
fn intersect_keeps_parent_when_other_later() {
    let parent = Deadline::after(Duration::from_secs(1));
    let later = Deadline::after(Duration::from_secs(60));
    let child = parent.intersect(&later);
    assert_eq!(child.instant(), parent.instant());
}

#[test]
fn intersect_with_never_keeps_self() {
    let parent = Deadline::after(Duration::from_secs(5));
    let child = parent.intersect(&Deadline::never());
    assert_eq!(child.instant(), parent.instant());
}

#[test]
fn intersect_never_with_finite_picks_finite() {
    let never = Deadline::never();
    let finite = Deadline::after(Duration::from_secs(5));
    let out = never.intersect(&finite);
    assert_eq!(out.instant(), finite.instant());
}

#[test]
fn intersect_two_nevers_is_never() {
    let a = Deadline::never();
    let b = Deadline::never();
    let out = a.intersect(&b);
    assert!(out.is_never());
}

#[test]
fn intersect_preserves_created_at_for_elapsed() {
    let parent = Deadline::after(Duration::from_secs(60));
    sleep(Duration::from_millis(20));
    let child = parent.intersect(&Deadline::after(Duration::from_secs(1)));
    // elapsed should reflect parent's start, not child's
    assert!(child.elapsed() >= Duration::from_millis(15));
}

#[test]
fn intersect_after_is_sugar_for_intersect_with_after() {
    let parent = Deadline::after(Duration::from_secs(60));
    let child = parent.intersect_after(Duration::from_millis(100));
    assert!(child.remaining() <= Duration::from_millis(100));
}

// ---------- equality ----------

#[test]
fn deadlines_with_same_at_are_equal() {
    let target = Instant::now() + Duration::from_secs(5);
    let a = Deadline::at(target);
    let b = Deadline::at(target);
    assert_eq!(a, b);
}

#[test]
fn two_nevers_are_equal() {
    assert_eq!(Deadline::never(), Deadline::never());
}

#[test]
fn never_is_not_equal_to_finite() {
    let finite = Deadline::after(Duration::from_secs(1));
    assert_ne!(Deadline::never(), finite);
}

// ---------- copy / clone ----------

#[test]
fn deadline_is_copy() {
    let d = Deadline::after(Duration::from_secs(5));
    let copied = d;
    // both still usable, no move
    assert_eq!(d.instant(), copied.instant());
}

// ---------- serde (feature-gated) ----------

#[cfg(feature = "serde")]
#[test]
fn serde_round_trips_never() {
    let d = Deadline::never();
    let s = serde_json::to_string(&d).unwrap();
    let back: Deadline = serde_json::from_str(&s).unwrap();
    assert!(back.is_never());
}

#[cfg(feature = "serde")]
#[test]
fn serde_round_trips_finite_snapshot() {
    let d = Deadline::after(Duration::from_secs(5));
    let s = serde_json::to_string(&d).unwrap();
    let back: Deadline = serde_json::from_str(&s).unwrap();
    // Snapshot semantics: deserialized deadline fires roughly the same
    // remaining duration from "now", which we approximate within 1s.
    assert!(!back.is_never());
    let diff = if back.remaining() > d.remaining() {
        back.remaining() - d.remaining()
    } else {
        d.remaining() - back.remaining()
    };
    assert!(diff < Duration::from_secs(1), "diff was {diff:?}");
}
