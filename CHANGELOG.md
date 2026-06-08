# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `Hash` implementation for `Deadline`, consistent with its `PartialEq`/`Eq`
  (hashed by fire instant only), so a `Deadline` can be used as a key in a
  `HashMap`/`HashSet`.
- `#![forbid(unsafe_code)]` at the crate root.
- A runnable `examples/agent_loop.rs` showing the intended agent-loop usage.
- Expanded README with an API reference table, a feature-flag section, and an
  agent-loop example.

## [0.1.0]

### Added
- Initial release: cooperative per-task `Deadline` primitive with
  `after` / `at` / `never` constructors, `check_or_err`, `expired`,
  `remaining` / `remaining_seconds`, `elapsed`, `instant`, `is_never`,
  `intersect` / `intersect_after`, and the `DeadlineExceeded` error.
- Optional `serde` feature (snapshot serialization of the remaining duration).
