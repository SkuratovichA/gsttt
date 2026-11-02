//! A file nested deeper than a thread's stack is read, or reported as one failed file, and the run goes on.

use gsttt_scenario::scenario;

use super::support::{Sandbox, init_repo, write};

/// How many levels each deep file nests: about twice what a worker's 2 MiB of stack holds in a debug build.
const LEVELS: usize = 5_000;

/// The text of a statement that nests `LEVELS` parentheses around `1`, closed or not.
fn parens(prefix: &str, closed: bool) -> String {
    let close = if closed {
        ")".repeat(LEVELS)
    } else {
        String::new()
    };
    format!("{prefix}{}1{close};\n", "(".repeat(LEVELS))
}

#[scenario("indexing", "A file nested deeper than a stack does not end the run")]
#[test]
fn a_file_nested_deeper_than_a_stack_does_not_end_the_run() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    init_repo(dir);
    write(
        dir,
        "Cargo.toml",
        "[package]\nname = \"deep\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    );
    write(dir, "src/lib.rs", "pub fn normal() {}\n");
    write(dir, "src/main.rs", "fn main() {}\n");
    write(dir, "package.json", "{\"name\": \"deep\"}\n");
    write(
        dir,
        "ok.ts",
        "import { b } from './b';\nexport const a = b;\n",
    );
    write(dir, "b.ts", "export const b = 1;\n");
    write(dir, "ok.py", "import os\n");
    for (closed, tag) in [(true, "closed"), (false, "open")] {
        write(
            dir,
            &format!("deep_{tag}.ts"),
            &parens("export const x = ", closed),
        );
        write(dir, &format!("deep_{tag}.py"), &parens("x = ", closed));
        write(
            dir,
            &format!("src/deep_{tag}.rs"),
            &parens("pub const X: i32 = ", closed),
        );
    }
    write(
        dir,
        "src/lib.rs",
        "pub fn normal() {}\nmod deep_closed;\nmod deep_open;\n",
    );

    let first = sandbox.run(&["index"]);
    assert_eq!(first.code, Some(0), "{first:?}");
    // One edited file makes the second run read the deep files back from the snapshot instead of parsing them.
    write(dir, "b.ts", "export const b = 2;\n");
    let second = sandbox.run(&["index"]);
    assert_eq!(second.code, Some(0), "{second:?}");
    assert!(
        second.stdout.contains(": 1 parsed, 12 reused."),
        "{second:?}"
    );
    assert!(
        second.stdout.contains("rust read 3 of 5 files"),
        "{second:?}"
    );

    let impact = sandbox.run(&["impact", "b.ts"]);
    assert_eq!(impact.code, Some(0), "{impact:?}");
    assert!(impact.stdout.contains("ok.ts"), "{impact:?}");
}
