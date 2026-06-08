//! A runnable sketch of how an agent loop uses a [`Deadline`].
//!
//! Run it with:
//!
//! ```text
//! cargo run --example agent_loop
//! ```
//!
//! The loop simulates an agent that chains several "steps" (each standing in
//! for an LLM call plus a tool call). A single wall-clock cap is set up front
//! with [`Deadline::after`]. Before every step the loop calls
//! [`Deadline::check_or_err`] and bails out with a partial result the moment the
//! cap is passed, instead of running forever. Each step is also given the
//! remaining time as its own per-call timeout via [`Deadline::remaining`].

use agent_deadline::Deadline;
use std::time::Duration;

/// Stand-in for one agent step (an LLM call + a tool call). Returns the
/// "answer" the step produced. In real code this would take `step_timeout` and
/// pass it to the network client.
fn run_step(index: usize, step_timeout: Duration) -> String {
    // Pretend the work takes a little while.
    std::thread::sleep(Duration::from_millis(40));
    format!(
        "step {index} done (had {:.3}s of budget left)",
        step_timeout.as_secs_f64()
    )
}

fn main() {
    // Cap the whole task at 150ms so the example terminates quickly while still
    // demonstrating the deadline firing mid-run.
    let deadline = Deadline::after(Duration::from_millis(150));

    let mut transcript: Vec<String> = Vec::new();

    for step in 0..100 {
        // Stop voluntarily once the wall-clock cap is passed.
        if let Err(exceeded) = deadline.check_or_err() {
            println!("hit the deadline: {exceeded}");
            break;
        }

        // Hand the remaining budget to the step as its per-call timeout.
        let remaining = deadline.remaining();
        let result = run_step(step, remaining);
        println!("{result}");
        transcript.push(result);
    }

    println!(
        "\ncompleted {} step(s) in {:.3}s before stopping",
        transcript.len(),
        deadline.elapsed().as_secs_f64()
    );
}
