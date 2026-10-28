//! Repository tasks, run as `cargo xtask <task>`.
//!
//! - `spec`: the spec gate ([`spec`]).
//! - `version`, `package`, `notes`, `check`: releases and repository checks ([`dist`]).
//! - `bench`: the speed comparison with the tools people use today ([`bench`](mod@bench)).
//!
//! [`root`] finds the repository a task works on, and [`digest`] checks that a commit id or checksum is hexadecimal.

mod bench;
mod digest;
mod dist;
mod root;
mod spec;

use std::process::ExitCode;

fn main() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((task, rest)) if task == "spec" => spec::run(rest),
        Some((task, rest)) if task == "bench" => bench::run(rest),
        Some((task, rest)) if dist::TASKS.contains(&task.as_str()) => dist::run(task, rest),
        _ => {
            anyhow::bail!("usage: cargo xtask <spec|version|package|notes|check|bench> [arguments]")
        }
    }
}
