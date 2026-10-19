# agent-deadline

[![Crates.io](https://img.shields.io/crates/v/agent-deadline.svg)](https://crates.io/crates/agent-deadline)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

Cooperative per-task deadline primitive for AI agent workflows.

Agent loops chain LLM calls and tool calls. Each step has its own network
timeout, but rarely is there a single wall-clock cap on the whole task.
This crate is a small zero-dependency `Deadline` type that the loop checks
between steps and hands to downstream calls as their remaining timeout.

Cooperative model: code checks the deadline. Nothing is preempted.

## Install

```toml
[dependencies]
agent-deadline = "0.1"
```

## Use

```rust
use std::time::Duration;
use agent_deadline::{Deadline, DeadlineExceeded};

let deadline = Deadline::after(Duration::from_secs(30));

// Between steps, check whether the cap has passed.
deadline.check_or_err().unwrap();

// Plug the remaining time into per-call timeouts.
let remaining = deadline.remaining();

// Tighten for a nested operation: pick the earlier of the two.
let retry = deadline.intersect(&Deadline::after(Duration::from_secs(5)));
assert!(retry.remaining() <= Duration::from_secs(5));
```

`check_or_err()` returns `Err(DeadlineExceeded)` once the clock has passed
the deadline. Catch it at the top of the loop to return a partial result
instead of hanging on the next call.

## In an agent loop

The intended pattern: set one wall-clock cap for the whole task, check it at
every loop boundary, and pass the remaining budget down to each call.

```rust
use std::time::Duration;
use agent_deadline::Deadline;

fn run_agent() -> Vec<String> {
    let deadline = Deadline::after(Duration::from_secs(30));
    let mut transcript = Vec::new();

    loop {
        // Stop voluntarily the moment the cap is passed.
        if deadline.check_or_err().is_err() {
            break;
        }

        // Give the step the remaining time as its own timeout.
        let _step_timeout = deadline.remaining();
        // let answer = call_llm_with_timeout(_step_timeout);
        // transcript.push(answer);
        // if done { break; }
        break; // placeholder for the doctest
    }

    transcript
}

let _ = run_agent();
```

A runnable version lives in [`examples/agent_loop.rs`](examples/agent_loop.rs):

```text
cargo run --example agent_loop
```

## API

| Constructor | Meaning |
| --- | --- |
| `Deadline::after(duration)` | Fires `duration` from now. Overflow (e.g. `Duration::MAX`) falls back to `never`. |
| `Deadline::at(instant)` | Fires at an absolute `Instant`. |
| `Deadline::never()` | Never fires. Also the `Default`. |

| Method | Returns |
| --- | --- |
| `check_or_err()` | `Ok(())`, or `Err(DeadlineExceeded)` once passed. No-op for `never`. |
| `expired()` | `true` once the clock has passed the deadline (`false` for `never`). |
| `remaining()` | Time left, saturating at `Duration::ZERO` (`Duration::MAX` for `never`). |
| `remaining_seconds()` | Time left as `f64` seconds (`f64::INFINITY` for `never`). |
| `elapsed()` | Time since the deadline was created. |
| `instant()` | The fire `Instant`, or `None` for `never`. |
| `is_never()` | `true` for the `never` deadline. |
| `intersect(&other)` | A deadline at the earlier of the two; `never` is the identity. Preserves `self`'s creation time so `elapsed()` keeps measuring from the original start. |
| `intersect_after(duration)` | Sugar for `intersect(&Deadline::after(duration))`. |

`Deadline` is `Copy`, `Eq`, and `Hash` (compared and hashed by fire instant
only, so it can be used as a `HashMap`/`HashSet` key). `DeadlineExceeded`
implements `std::error::Error` and `Display`, and carries the elapsed time
(`elapsed` / `elapsed_seconds()`).

## Features

- `serde` (off by default): derive-free `Serialize`/`Deserialize` for
  `Deadline`. Serialization is a *snapshot* of the remaining duration
  (`None` for `never`); deserializing reconstructs a deadline that fires
  roughly the same remaining time from the new "now".

```toml
[dependencies]
agent-deadline = { version = "0.1", features = ["serde"] }
```

The crate forbids `unsafe` code (`#![forbid(unsafe_code)]`) and has no required
dependencies.

## License

MIT
