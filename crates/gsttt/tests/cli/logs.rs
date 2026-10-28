//! Logs go to stderr, filtered by `GSTTT_LOG`, and never to stdout.

use super::support::{Sandbox, ran};

#[test]
fn logs_show_only_when_gsttt_log_asks_for_them() {
    let sandbox = Sandbox::new();
    let quiet = sandbox.run(&["completions", "zsh"]);
    assert_eq!(quiet.code, Some(0));
    assert_eq!(quiet.stderr, "", "only errors show by default");

    let mut command = sandbox.gsttt();
    command
        .env("GSTTT_LOG", "debug")
        .args(["completions", "zsh"]);
    let loud = ran(&mut command);
    assert_eq!(loud.code, Some(0));
    assert_eq!(loud.stdout, quiet.stdout, "the answer is the same");
    assert!(loud.stderr.contains("DEBUG"), "{}", loud.stderr);
    assert!(loud.stderr.contains("completions"), "{}", loud.stderr);

    let mut command = sandbox.gsttt();
    command
        .env("GSTTT_LOG", "warn")
        .args(["completions", "zsh"]);
    assert_eq!(ran(&mut command).stderr, "", "debug lines are below warn");
}

#[test]
fn a_failure_logs_nothing_to_stdout() {
    let sandbox = Sandbox::new();
    let mut command = sandbox.gsttt();
    command
        .env("GSTTT_LOG", "trace")
        .args(["--json", "coverage"]);
    let ran = ran(&mut command);
    assert_eq!(ran.code, Some(1));
    assert_eq!(ran.stdout, "");
    assert!(
        ran.stderr.contains("TRACE") || ran.stderr.contains("DEBUG"),
        "{}",
        ran.stderr
    );
}
