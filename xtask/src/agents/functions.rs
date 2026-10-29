//! The exported functions of a scope and the files that refer to each, as `bench/agents/functions.mjs` lists them.

use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, ensure};
use serde::Deserialize;

/// How many lines of what the script printed to its standard error an error carries.
const ERROR_LINES: usize = 40;

/// A function a file of the scope exports, which a question can be about.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Candidate {
    /// The file that exports it, relative to the checkout.
    pub(super) file: String,
    pub(super) name: String,
    /// The line of its name, from 1.
    pub(super) line: u32,
    /// The offset in bytes just after the opening brace of its body; absent when the body is an expression, and for a generator, whose body runs at its first `next()` and not when it is called.
    pub(super) body_offset: Option<usize>,
    /// The other files of the scope that refer to it, as the language service finds them.
    pub(super) users: Vec<String>,
}

/// What the script prints; it also prints the version of TypeScript and the number of files, which are for readers.
#[derive(Debug, Deserialize)]
struct Listing {
    functions: Vec<Candidate>,
}

/// Runs the script with the `node` on the PATH and the TypeScript installed in `tools`, over the tracked files of `scope` in `checkout`.
pub(super) fn list(
    script: &Path,
    tools: &Path,
    checkout: &Path,
    scope: &str,
    tsconfig: &str,
) -> anyhow::Result<Vec<Candidate>> {
    let output = Command::new("node")
        .arg(script)
        .arg("--tools")
        .arg(tools)
        .arg("--checkout")
        .arg(checkout)
        .args(["--scope", scope, "--tsconfig", tsconfig])
        .output()
        .context("cannot run node")?;
    ensure!(
        output.status.success(),
        "{} failed:\n{}",
        script.display(),
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .take(ERROR_LINES)
            .collect::<Vec<_>>()
            .join("\n")
    );
    let listing: Listing = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{} printed no list of functions", script.display()))?;
    Ok(listing.functions)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::agents::say;
    use crate::bench::plan::Programs;
    use crate::dist::git::git;
    use crate::root::repository_root;

    #[test]
    fn the_listing_of_the_script_is_read() {
        let text = r#"{"typescript":"5.9.3","files":2,"functions":[
            {"file":"a.ts","name":"f","line":3,"bodyOffset":21,"users":["b.ts"]},
            {"file":"a.ts","name":"g","line":9,"bodyOffset":null,"users":[]}]}"#;
        let listing: Listing = serde_json::from_str(text).unwrap();
        assert_eq!(listing.functions.len(), 2);
        assert_eq!(
            listing.functions.first().and_then(|c| c.body_offset),
            Some(21)
        );
        assert_eq!(listing.functions.last().and_then(|c| c.body_offset), None);
    }

    /// The script as `cargo xtask agents questions` runs it, over a git repository of a generator, an arrow function and a function: all three are listed, and only the function has a body to put a throw in. The script needs the Node tools of `bench/tools` and a `node`; where they are not installed, as in CI, the test says so and ends.
    #[test]
    fn a_generator_has_no_body_offset_and_a_function_has_one() {
        let root = repository_root().unwrap();
        let programs = Programs::new(&root.join("bench/tools"), Path::new(""));
        let script = root.join("bench/agents/functions.mjs");
        let typescript = programs.node.join("node_modules/typescript/package.json");
        if !typescript.is_file() {
            say(&format!(
                "skipped: {} is not installed; `cargo xtask agents questions` installs the Node tools",
                typescript.display()
            ))
            .unwrap();
            return;
        }
        if Command::new("node").arg("--version").output().is_err() {
            say("skipped: there is no `node` on the PATH").unwrap();
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let checkout = fs::canonicalize(dir.path()).unwrap();
        fs::write(
            checkout.join("tsconfig.json"),
            r#"{"compilerOptions":{"target":"es2022","module":"esnext","moduleResolution":"bundler","strict":true}}"#,
        )
        .unwrap();
        fs::write(
            checkout.join("lib.ts"),
            "export function* g() {}\nexport const e = () => 1\nexport function f() {}\n",
        )
        .unwrap();
        git(&checkout, &["init", "--quiet"]).unwrap();
        git(&checkout, &["add", "."]).unwrap();

        let found = list(&script, &programs.node, &checkout, ".", "tsconfig.json").unwrap();
        let shapes: Vec<(&str, bool)> = found
            .iter()
            .map(|candidate| (candidate.name.as_str(), candidate.body_offset.is_some()))
            .collect();
        assert_eq!(shapes, [("e", false), ("f", true), ("g", false)]);
    }
}
