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

## License

MIT
