//! The candidates of a scope that the scripts of `bench/agents/` list: the exported functions with the files that refer to each (`functions.mjs`), and the methods that a resolver calls with the files that send an operation it serves (`clients.mjs`).

use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, ensure};
use serde::Deserialize;

/// How many lines of what the script printed to its standard error an error carries.
const ERROR_LINES: usize = 40;

/// A function a file of the scope exports, or a method a resolver calls, which a question can be about.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Candidate {
    /// The file that exports the function, or defines the class of the method, relative to the checkout.
    pub(super) file: String,
    /// The name of the function, or `Class.method`.
    pub(super) name: String,
    /// The line of its name, from 1.
    pub(super) line: u32,
    /// The offset in bytes just after the opening brace of its body; absent when the body is an expression, and for a generator, whose body runs at its first `next()` and not when it is called.
    pub(super) body_offset: Option<usize>,
    /// The other files of the scope that refer to a function, as the language service finds them; absent for a method.
    #[serde(default)]
    pub(super) users: Vec<String>,
    /// The files of the scope that hold an operation whose resolver calls a method; absent for a function.
    #[serde(default)]
    pub(super) clients: Vec<String>,
}

/// What a script prints: `functions.mjs` lists `functions` and `clients.mjs` lists `methods`; they also print the versions of their tools and the number of files, which are for readers.
#[derive(Debug, Deserialize)]
struct Listing {
    #[serde(alias = "methods")]
    functions: Vec<Candidate>,
}

/// Runs the script with the `node` on the PATH and the tools installed in `tools`, over the tracked files of `scope` in `checkout`.
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
        .with_context(|| format!("{} printed no list of candidates", script.display()))?;
    Ok(listing.functions)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

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

    #[test]
    fn the_listing_of_clients_names_its_candidates_methods() {
        let text = r#"{"typescript":"5.9.3","graphql":"17.0.2","files":2,"methods":[
            {"file":"s.ts","name":"S.m","line":3,"clients":["a.graphql","b.ts"]}]}"#;
        let listing: Listing = serde_json::from_str(text).unwrap();
        let method = listing.functions.first().unwrap();
        assert_eq!(method.name, "S.m");
        assert_eq!(method.clients, ["a.graphql", "b.ts"]);
        assert!(method.users.is_empty() && method.body_offset.is_none());
    }

    /// A git repository of a NestJS-like server and of the documents that call it, run through `clients.mjs` the way `cargo xtask agents questions` runs it; `None` where the Node tools or `node` are missing, as in CI, after saying so. The decorators come from a package that is not installed: the script reads them by their import, and the language service follows only the relative imports.
    fn clients_fixture(
        documents: &[(&str, &str)],
    ) -> Option<(tempfile::TempDir, PathBuf, PathBuf)> {
        let root = repository_root().unwrap();
        let programs = Programs::new(&root.join("bench/tools"), Path::new(""));
        let graphql = programs.node.join("node_modules/graphql/package.json");
        if !graphql.is_file() {
            say(&format!(
                "skipped: {} is not installed; `cargo xtask agents questions` installs the Node tools",
                graphql.display()
            ))
            .unwrap();
            return None;
        }
        if Command::new("node").arg("--version").output().is_err() {
            say("skipped: there is no `node` on the PATH").unwrap();
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let files = [
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"target":"es2022","module":"esnext","moduleResolution":"bundler","experimentalDecorators":true}}"#,
            ),
            (
                "server/service.ts",
                "export class TeamService {\n  join() { return 1 }\n  members() { return 2 }\n  info() { return 3 }\n  orphan() { return 4 }\n  helper() { return 5 }\n  unused() { return 6 }\n}\n",
            ),
            (
                "server/other.ts",
                "import { TeamService } from './service'\nexport class Other {\n  run(service: TeamService) { return service.helper() }\n}\n",
            ),
            (
                "server/resolver.ts",
                "import { Mutation, Query as Read, Resolver } from '@nestjs/graphql'\nimport { TeamService } from './service'\n\n@Resolver()\nexport class TeamResolver {\n  constructor(private readonly service: TeamService) {}\n\n  @Mutation(() => Boolean, { name: 'addMember' })\n  joinTeam() { return this.service.join() }\n\n  @Read(() => Boolean)\n  members() { return this.service.members() }\n\n  @Read(() => Boolean, { name: 'teamInfo' })\n  describe() { return this.service.info() }\n\n  @Mutation(() => Boolean)\n  orphan() { return this.service.orphan() }\n}\n",
            ),
            ("client/join.graphql", "mutation Join { addMember }\n"),
            // A query of the same name is another field than the mutation.
            ("client/query.graphql", "query Same { addMember }\n"),
            (
                "client/alias.ts",
                "import gql from 'graphql-tag'\nexport const query = gql`query Team { team: members }`\n",
            ),
            (
                "client/fragment.graphql",
                "fragment TeamInfo on Query { teamInfo }\nquery Info { ...TeamInfo }\n",
            ),
        ];
        for (path, text) in files.iter().chain(documents) {
            let file = checkout.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        git(&checkout, &["init", "--quiet"]).unwrap();
        git(&checkout, &["add", "."]).unwrap();
        Some((dir, root.join("bench/agents/clients.mjs"), programs.node))
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The clients of a method are the files that select the field of a resolver that calls it"
    )]
    #[test]
    fn the_clients_of_a_method_select_the_field_of_a_resolver_that_calls_it() {
        let Some((dir, script, tools)) = clients_fixture(&[]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let found = list(&script, &tools, &checkout, ".", "tsconfig.json").unwrap();
        let listed: Vec<(&str, &str, Vec<&str>)> = found
            .iter()
            .map(|method| {
                (
                    method.file.as_str(),
                    method.name.as_str(),
                    method.clients.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        // `helper` is called by a class that is no resolver and `unused` by none, so neither is listed; a field no document selects lists no client; a mutation is not selected by a query of its name, an alias selects the field it aliases, and a fragment spread selects what the fragment does.
        assert_eq!(
            listed,
            [
                (
                    "server/service.ts",
                    "TeamService.info",
                    vec!["client/fragment.graphql"]
                ),
                (
                    "server/service.ts",
                    "TeamService.join",
                    vec!["client/join.graphql"]
                ),
                (
                    "server/service.ts",
                    "TeamService.members",
                    vec!["client/alias.ts"]
                ),
                ("server/service.ts", "TeamService.orphan", vec![]),
            ]
        );
        assert_eq!(
            found.iter().map(|method| method.line).collect::<Vec<_>>(),
            [4, 2, 3, 5]
        );
    }

    #[test]
    fn a_document_that_does_not_parse_stops_the_oracle() {
        let Some((dir, script, tools)) = clients_fixture(&[("client/broken.graphql", "query {\n")])
        else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let error = list(&script, &tools, &checkout, ".", "tsconfig.json")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("client/broken.graphql:1") && error.contains("does not parse"),
            "{error}"
        );
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
