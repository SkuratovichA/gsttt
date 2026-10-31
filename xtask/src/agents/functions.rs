//! The candidates of a scope that the scripts of `bench/agents/` list: the exported functions with the files that refer to each (`functions.mjs`), and the methods that a resolver calls with the files that send an operation it serves (`clients.mjs`).

use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, ensure};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// How many lines of what the script printed to its standard error an error carries.
const ERROR_LINES: usize = 40;

/// A function a file of the scope exports, or a method a resolver calls, which a question can be about.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    /// The file that exports the function, or defines the class of the method, relative to the checkout.
    pub(super) file: String,
    /// The name of the function, or `Class.method`.
    pub(super) name: String,
    /// The line of its name, from 1.
    pub(super) line: u32,
    /// The offset in bytes just after the opening brace of its body; absent when the body is an expression, and for a generator, whose body runs at its first `next()` and not when it is called; absent for a method.
    pub(super) body_offset: Option<usize>,
    /// The other files of the scope that refer to a function, as the language service finds them; empty for a method.
    pub(super) users: Vec<String>,
    /// The files of the scope that hold an operation whose resolver calls a method; empty for a function.
    pub(super) clients: Vec<String>,
    /// Whether a field that a resolver calling the method serves is also served by a resolver of another root type, as `Query foo` and `Mutation foo`: a name alone does not tell the two apart, so no question is about such a method. Always false for a function.
    pub(super) ambiguous: bool,
}

/// What `functions.mjs` prints for a function; it also prints the versions of its tools and the number of files, which are for readers.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Function {
    file: String,
    name: String,
    line: u32,
    body_offset: Option<usize>,
    users: Vec<String>,
}

/// What `functions.mjs` prints.
#[derive(Debug, Deserialize)]
struct FunctionListing {
    functions: Vec<Function>,
}

/// What `clients.mjs` prints for a method that a resolver calls.
#[derive(Debug, Deserialize)]
struct Method {
    file: String,
    name: String,
    line: u32,
    clients: Vec<String>,
    ambiguous: bool,
}

/// What `clients.mjs` prints.
#[derive(Debug, Deserialize)]
struct MethodListing {
    methods: Vec<Method>,
}

impl From<Function> for Candidate {
    fn from(function: Function) -> Self {
        Self {
            file: function.file,
            name: function.name,
            line: function.line,
            body_offset: function.body_offset,
            users: function.users,
            clients: Vec::new(),
            ambiguous: false,
        }
    }
}

impl From<Method> for Candidate {
    fn from(method: Method) -> Self {
        Self {
            file: method.file,
            name: method.name,
            line: method.line,
            body_offset: None,
            users: Vec::new(),
            clients: method.clients,
            ambiguous: method.ambiguous,
        }
    }
}

/// The exported functions of the scope, as `functions.mjs` lists them.
pub(super) fn list_functions(
    script: &Path,
    tools: &Path,
    checkout: &Path,
    scope: &str,
    tsconfig: &str,
) -> anyhow::Result<Vec<Candidate>> {
    let listing: FunctionListing = run(script, tools, checkout, scope, tsconfig)?;
    Ok(listing.functions.into_iter().map(Candidate::from).collect())
}

/// The methods that a resolver of the scope calls, as `clients.mjs` lists them.
pub(super) fn list_methods(
    script: &Path,
    tools: &Path,
    checkout: &Path,
    scope: &str,
    tsconfig: &str,
) -> anyhow::Result<Vec<Candidate>> {
    let listing: MethodListing = run(script, tools, checkout, scope, tsconfig)?;
    Ok(listing.methods.into_iter().map(Candidate::from).collect())
}

/// Runs the script with the `node` on the PATH and the tools installed in `tools`, over the tracked files of `scope` in `checkout`, and reads what it prints.
fn run<T: DeserializeOwned>(
    script: &Path,
    tools: &Path,
    checkout: &Path,
    scope: &str,
    tsconfig: &str,
) -> anyhow::Result<T> {
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
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{} printed no list of candidates", script.display()))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
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
        let listing: FunctionListing = serde_json::from_str(text).unwrap();
        let found: Vec<Candidate> = listing.functions.into_iter().map(Candidate::from).collect();
        assert_eq!(found.len(), 2);
        assert_eq!(found.first().and_then(|c| c.body_offset), Some(21));
        assert_eq!(
            found.first().map(|c| c.users.as_slice()),
            Some(&["b.ts".to_owned()][..])
        );
        assert_eq!(found.last().and_then(|c| c.body_offset), None);
        assert!(found.iter().all(|c| c.clients.is_empty() && !c.ambiguous));
    }

    #[test]
    fn the_listing_of_clients_names_its_candidates_methods() {
        let text = r#"{"typescript":"5.9.3","graphql":"17.0.2","files":2,"methods":[
            {"file":"s.ts","name":"S.m","line":3,"clients":["a.graphql","b.ts"],"ambiguous":true}]}"#;
        let listing: MethodListing = serde_json::from_str(text).unwrap();
        let method = Candidate::from(listing.methods.into_iter().next().unwrap());
        assert_eq!(method.name, "S.m");
        assert_eq!(method.clients, ["a.graphql", "b.ts"]);
        assert!(method.ambiguous);
        assert!(method.users.is_empty() && method.body_offset.is_none());
    }

    #[test]
    fn a_listing_leaves_out_nothing_a_script_prints() {
        // Each script prints every field of its candidates; a field it forgot is an error and not an empty list.
        let functions = r#"{"functions":[{"file":"a.ts","name":"f","line":3,"bodyOffset":21}]}"#;
        assert!(serde_json::from_str::<FunctionListing>(functions).is_err());
        let methods = r#"{"methods":[{"file":"s.ts","name":"S.m","line":3,"clients":[]}]}"#;
        assert!(serde_json::from_str::<MethodListing>(methods).is_err());
        let renamed = r#"{"functions":[{"file":"s.ts","name":"S.m","line":3,"clients":[],"ambiguous":false}]}"#;
        assert!(serde_json::from_str::<MethodListing>(renamed).is_err());
    }

    /// The programs of the Node tools of `bench/tools` when each of the `packages` and a `node` are there, to run a script of `bench/agents/` with. Where they are not, a developer's machine says so and the test ends; in CI, which installs them with `cargo xtask agents tools` before the tests, a test that ended there would count as proof of a scenario it never ran, so it fails.
    fn node_tools(packages: &[&str]) -> Option<Programs> {
        let root = repository_root().unwrap();
        let programs = Programs::new(&root.join("bench/tools"), Path::new(""));
        let missing = packages
            .iter()
            .map(|package| {
                programs
                    .node
                    .join("node_modules")
                    .join(package)
                    .join("package.json")
            })
            .find(|manifest| !manifest.is_file())
            .map(|manifest| format!("{} is not installed", manifest.display()))
            .or_else(|| {
                Command::new("node")
                    .arg("--version")
                    .output()
                    .is_err()
                    .then(|| "there is no `node` on the PATH".to_owned())
            });
        let Some(missing) = missing else {
            return Some(programs);
        };
        assert!(
            std::env::var_os("CI").is_none(),
            "{missing}: CI installs the Node tools with `cargo xtask agents tools` before the tests"
        );
        say(&format!(
            "skipped: {missing}; `cargo xtask agents tools` installs the Node tools"
        ))
        .unwrap();
        None
    }

    /// The methods of `TeamService`, one per line from the second, so that the line a question names is found in this list.
    const SERVICE_METHODS: [&str; 13] = [
        "join",
        "members",
        "info",
        "orphan",
        "helper",
        "unused",
        "byString",
        "watch",
        "crossFragment",
        "gqlFile",
        "templated",
        "sharedA",
        "sharedB",
    ];

    fn service_text() -> String {
        let mut text = String::from("export class TeamService {\n");
        for (number, method) in SERVICE_METHODS.iter().enumerate() {
            writeln!(text, "  {method}() {{ return {number} }}").unwrap();
        }
        text.push_str("}\n");
        text
    }

    /// The line of the method `name` of `TeamService`, from 1.
    fn service_line(name: &str) -> u32 {
        let position = SERVICE_METHODS
            .iter()
            .position(|method| *method == name)
            .unwrap();
        u32::try_from(position).unwrap() + 2
    }

    const RESOLVER: &str = r"import { Mutation, Query as Read, Resolver, Subscription } from '@nestjs/graphql'
import { Other } from './other'
import { TeamService } from './service'

@Resolver()
export class TeamResolver {
  constructor(private readonly service: TeamService, private readonly other: Other) {}

  @Mutation(() => Boolean, { name: 'addMember' })
  joinTeam() { return this.service.join() }

  @Read(() => Boolean)
  members() { return this.service.members() }

  @Read(() => Boolean, { name: 'teamInfo' })
  describe() { return this.service.info() }

  @Mutation(() => Boolean)
  orphan() { return this.service.orphan() }

  @Read('byName')
  lookup() { return this.service.byString() }

  @Subscription(() => Boolean)
  watch() { return this.service.watch() }

  @Read(() => Boolean)
  cross() { return this.service.crossFragment() }

  @Read(() => Boolean)
  fromGql() { return this.service.gqlFile() }

  @Read(() => Boolean)
  templated() { return this.service.templated() }

  @Read(() => Boolean)
  relay() { return this.other.run(this.service) }

  @Read(() => Boolean, { name: 'shared' })
  sharedQuery() { return this.service.sharedA() }

  @Mutation(() => Boolean, { name: 'shared' })
  sharedMutation() { return this.service.sharedB() }
}
";

    const OTHER: &str = "import { TeamService } from './service'\nexport class Other {\n  run(service: TeamService) { return service.helper() }\n}\n";

    /// A git repository of a NestJS-like server and of the documents that call it, run through `clients.mjs` the way `cargo xtask agents questions` runs it, with the `extra` files written over or beside its own; `None` where the Node tools or `node` are missing, as `node_tools` says. The decorators come from a package that is not installed: the script reads them by their import, and the language service follows only the relative imports.
    fn clients_fixture(extra: &[(&str, &str)]) -> Option<(tempfile::TempDir, PathBuf, PathBuf)> {
        let programs = node_tools(&["typescript", "graphql"])?;
        let service = service_text();
        let files = [
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"target":"es2022","module":"esnext","moduleResolution":"bundler","experimentalDecorators":true}}"#,
            ),
            ("server/service.ts", service.as_str()),
            ("server/other.ts", OTHER),
            ("server/resolver.ts", RESOLVER),
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
            // The field is named by the string of the decorator and not by the method; a document that selects the method's name selects nothing.
            ("client/by_name.graphql", "query ByName { byName }\n"),
            ("client/decoy.graphql", "query Decoy { lookup }\n"),
            ("client/watch.graphql", "subscription Watch { watch }\n"),
            // A fragment of one file, spread by the operation of another: the operation's file is the client.
            (
                "client/cross_fragment.graphql",
                "fragment Cross on Query { cross }\n",
            ),
            ("client/cross_use.graphql", "query UseCross { ...Cross }\n"),
            ("client/from.gql", "query FromGql { fromGql }\n"),
            (
                "client/templated.ts",
                "import gql from 'graphql-tag'\nimport { details } from './details'\nexport const query = gql`query Templated { ...Details } ${details}`\n",
            ),
            (
                "client/details.ts",
                "import gql from 'graphql-tag'\nexport const details = gql`fragment Details on Query { templated }`\n",
            ),
            ("client/relay.graphql", "query Relay { relay }\n"),
            ("client/shared.graphql", "query ReadShared { shared }\n"),
            (
                "client/shared_write.graphql",
                "mutation WriteShared { shared }\n",
            ),
        ];
        let dir = tempfile::tempdir().unwrap();
        let checkout = fs::canonicalize(dir.path()).unwrap();
        for (path, text) in files.iter().chain(extra) {
            let file = checkout.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        git(&checkout, &["init", "--quiet"]).unwrap();
        git(&checkout, &["add", "."]).unwrap();
        let script = repository_root().unwrap().join("bench/agents/clients.mjs");
        Some((dir, script, programs.node))
    }

    /// What the script lists for the fixture, or what it should list.
    #[derive(Debug, PartialEq)]
    struct Listed {
        file: String,
        name: String,
        line: u32,
        clients: Vec<String>,
        ambiguous: bool,
    }

    fn listed(found: &[Candidate]) -> Vec<Listed> {
        found
            .iter()
            .map(|method| Listed {
                file: method.file.clone(),
                name: method.name.clone(),
                line: method.line,
                clients: method.clients.clone(),
                ambiguous: method.ambiguous,
            })
            .collect()
    }

    /// A method of `TeamService` that the script should list, with the files that should be its clients.
    fn service_method(name: &str, clients: &[&str], ambiguous: bool) -> Listed {
        Listed {
            file: "server/service.ts".to_owned(),
            name: format!("TeamService.{name}"),
            line: service_line(name),
            clients: clients.iter().map(|client| (*client).to_owned()).collect(),
            ambiguous,
        }
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
        let found = list_methods(&script, &tools, &checkout, ".", "tsconfig.json").unwrap();
        // `helper` is called by `Other.run` and not by a resolver, which calls `Other.run` and so is listed in its place, and `unused` by none, so neither is listed; a field no document selects lists no client; a mutation is not selected by a query of its name, an alias selects the field it aliases, a fragment spread selects what the fragment does whichever file defines it, a template with an expression and a `.gql` file are documents, a field named by the string of the decorator is selected by that name, a subscription is a root type, and a field served under two root types makes both methods ambiguous.
        let expected = [
            Listed {
                file: "server/other.ts".to_owned(),
                name: "Other.run".to_owned(),
                line: 3,
                clients: vec!["client/relay.graphql".to_owned()],
                ambiguous: false,
            },
            service_method("byString", &["client/by_name.graphql"], false),
            service_method("crossFragment", &["client/cross_use.graphql"], false),
            service_method("gqlFile", &["client/from.gql"], false),
            service_method("info", &["client/fragment.graphql"], false),
            service_method("join", &["client/join.graphql"], false),
            service_method("members", &["client/alias.ts"], false),
            service_method("orphan", &[], false),
            service_method("sharedA", &["client/shared.graphql"], true),
            service_method("sharedB", &["client/shared_write.graphql"], true),
            service_method("templated", &["client/templated.ts"], false),
            service_method("watch", &["client/watch.graphql"], false),
        ];
        assert_eq!(listed(&found), expected);
    }

    #[test]
    fn a_document_that_does_not_parse_stops_the_oracle() {
        let Some((dir, script, tools)) = clients_fixture(&[("client/broken.graphql", "query {\n")])
        else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let error = list_methods(&script, &tools, &checkout, ".", "tsconfig.json")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("client/broken.graphql:1") && error.contains("does not parse"),
            "{error}"
        );
    }

    /// A resolver file of the fixture's own beside the others, which the script must refuse, with the text the error should hold.
    fn stops_the_oracle(resolver: &str, says: &[&str]) {
        let Some((dir, script, tools)) = clients_fixture(&[("server/odd.ts", resolver)]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let error = list_methods(&script, &tools, &checkout, ".", "tsconfig.json")
            .unwrap_err()
            .to_string();
        for text in says {
            assert!(error.contains(text), "{text} is not in {error}");
        }
    }

    #[test]
    fn a_decorator_argument_the_script_cannot_read_stops_the_oracle() {
        let resolver = "import { Query, Resolver } from '@nestjs/graphql'\nconst options = { name: 'x' }\n\n@Resolver()\nexport class Odd {\n  @Query(() => Boolean, options)\n  odd() { return 1 }\n}\n";
        stops_the_oracle(resolver, &["server/odd.ts:6", "@Query", "options"]);
    }

    #[test]
    fn an_options_object_with_a_spread_stops_the_oracle() {
        let resolver = "import { Mutation, Resolver } from '@nestjs/graphql'\nconst options = { name: 'x' }\n\n@Resolver()\nexport class Odd {\n  @Mutation(() => Boolean, { ...options })\n  odd() { return 1 }\n}\n";
        stops_the_oracle(resolver, &["server/odd.ts:6", "@Mutation", "spread"]);
    }

    /// What the script prints with `--fields` for the fixture, as the JSON it is.
    fn list_fields(
        script: &Path,
        tools: &Path,
        checkout: &Path,
    ) -> anyhow::Result<serde_json::Value> {
        let output = Command::new("node")
            .arg(script)
            .arg("--tools")
            .arg(tools)
            .arg("--checkout")
            .arg(checkout)
            .args(["--scope", ".", "--tsconfig", "tsconfig.json", "--fields"])
            .output()
            .context("cannot run node")?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    /// The entry of `field` among the fields the script printed.
    fn field_entry(listing: &serde_json::Value, field: &str) -> serde_json::Value {
        listing["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["field"] == field)
            .unwrap_or_else(|| panic!("{field} is not in {listing}"))
            .clone()
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A namespace import of the decorators is read like named ones"
    )]
    #[test]
    fn a_namespace_import_of_the_decorators_is_read_like_named_ones() {
        let resolver = "import * as graphql from '@nestjs/graphql'\n\n@graphql.Resolver()\nexport class Spaced {\n  @graphql.Query(() => Boolean)\n  spaced() { return 1 }\n\n  @graphql.Mutation(() => Boolean, { name: 'renamed' })\n  other() { return 1 }\n}\n";
        let Some((dir, script, tools)) = clients_fixture(&[
            ("server/spaced.ts", resolver),
            ("client/spaced.graphql", "query Spaced { spaced }\n"),
        ]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let listing = list_fields(&script, &tools, &checkout).unwrap();
        assert_eq!(
            field_entry(&listing, "Query.spaced"),
            serde_json::json!({
                "field": "Query.spaced",
                "servers": [{"file": "server/spaced.ts", "line": 6, "method": "Spaced.spaced"}],
                "clients": ["client/spaced.graphql"],
            })
        );
        assert_eq!(
            field_entry(&listing, "Mutation.renamed")["servers"][0]["method"],
            "Spaced.other"
        );
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The fields flag lists the imports it could not follow in place of refusing"
    )]
    #[test]
    fn the_fields_flag_lists_the_imports_it_could_not_follow_in_place_of_refusing() {
        let Some((dir, script, tools)) = clients_fixture(&[(
            "server/dangling.ts",
            "import './missing'\nimport './absent'\n",
        )]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let listing = list_fields(&script, &tools, &checkout).unwrap();
        assert_eq!(
            listing["unresolved"],
            serde_json::json!([
                {"file": "server/dangling.ts", "line": 1, "specifier": "./missing"},
                {"file": "server/dangling.ts", "line": 2, "specifier": "./absent"},
            ])
        );
        assert!(listing["fields"].as_array().unwrap().len() > 1);
        // The answer by method uses references, so the same tree stops it.
        let error = list_methods(&script, &tools, &checkout, ".", "tsconfig.json")
            .unwrap_err()
            .to_string();
        assert!(error.contains("server/dangling.ts:1: ./missing"), "{error}");
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A field name the fields flag cannot read stops the oracle"
    )]
    #[test]
    fn a_field_name_the_fields_flag_cannot_read_stops_the_oracle() {
        let resolver = "import { Query, Resolver } from '@nestjs/graphql'\nimport { NAME } from './missing'\n\n@Resolver()\nexport class Named {\n  @Query(() => Boolean, { name: NAME })\n  named() { return 1 }\n}\n";
        let Some((dir, script, tools)) = clients_fixture(&[("server/named.ts", resolver)]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let error = list_fields(&script, &tools, &checkout)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("server/named.ts:6") && error.contains("not a string"),
            "{error}"
        );
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A call of gql or graphql with a template or a string is a document"
    )]
    #[test]
    fn a_call_of_graphql_with_a_template_or_a_string_is_a_document() {
        let client = "declare function graphql(source: string): unknown\nexport const a = graphql(`query A { fromCall }`)\nexport const b = graphql(\"query B { fromString }\")\nexport const c = graphql(`query C { ...Part } ${part}`)\nexport const d = graphql(name)\n";
        let Some((dir, script, tools)) = clients_fixture(&[
            ("client/codegen.ts", client),
            (
                "client/part.graphql",
                "fragment Part on Query { fromFragment }\n",
            ),
        ]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let listing = list_fields(&script, &tools, &checkout).unwrap();
        for field in ["fromCall", "fromString", "fromFragment"] {
            assert_eq!(
                field_entry(&listing, &format!("Query.{field}"))["clients"],
                serde_json::json!(["client/codegen.ts"]),
                "{field}"
            );
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The fields flag lists each root field with its servers and its clients"
    )]
    #[test]
    fn the_fields_flag_lists_each_root_field_with_its_servers_and_its_clients() {
        let Some((dir, script, tools)) = clients_fixture(&[]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        let listing = list_fields(&script, &tools, &checkout).unwrap();
        let line = |method: &str| {
            let at = RESOLVER
                .lines()
                .position(|l| l.starts_with(&format!("  {method}(")))
                .unwrap();
            at + 1
        };
        // A field served under two root types is two entries; a field only a document selects has no server; a field only a resolver serves has no client.
        assert_eq!(
            field_entry(&listing, "Mutation.shared"),
            serde_json::json!({
                "field": "Mutation.shared",
                "servers": [{"file": "server/resolver.ts", "line": line("sharedMutation"), "method": "TeamResolver.sharedMutation"}],
                "clients": ["client/shared_write.graphql"],
            })
        );
        assert_eq!(
            field_entry(&listing, "Query.lookup"),
            serde_json::json!({"field": "Query.lookup", "servers": [], "clients": ["client/decoy.graphql"]})
        );
        assert_eq!(
            field_entry(&listing, "Query.byName")["servers"],
            serde_json::json!([{"file": "server/resolver.ts", "line": line("lookup"), "method": "TeamResolver.lookup"}])
        );
        assert_eq!(
            field_entry(&listing, "Mutation.orphan")["clients"],
            serde_json::json!([])
        );
        let names: Vec<&str> = listing["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["field"].as_str().unwrap())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        assert!(listing.get("methods").is_none());
    }

    #[test]
    fn a_tracked_path_that_is_no_document_is_never_read() {
        let Some((dir, script, tools)) = clients_fixture(&[]) else {
            return;
        };
        let checkout = fs::canonicalize(dir.path()).unwrap();
        // A symlink to nothing, as a tracked submodule path or a link out of the tree is to a reader that opens it.
        std::os::unix::fs::symlink("missing", checkout.join("client/link.png")).unwrap();
        git(&checkout, &["add", "."]).unwrap();
        let found = list_methods(&script, &tools, &checkout, ".", "tsconfig.json").unwrap();
        assert_eq!(found.len(), 12);
    }

    /// The script as `cargo xtask agents questions` runs it, over a git repository of a generator, an arrow function and a function: all three are listed, and only the function has a body to put a throw in. The script needs the Node tools of `bench/tools` and a `node`, as `node_tools` says.
    #[test]
    fn a_generator_has_no_body_offset_and_a_function_has_one() {
        let Some(programs) = node_tools(&["typescript"]) else {
            return;
        };
        let script = repository_root()
            .unwrap()
            .join("bench/agents/functions.mjs");
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

        let found =
            list_functions(&script, &programs.node, &checkout, ".", "tsconfig.json").unwrap();
        let shapes: Vec<(&str, bool)> = found
            .iter()
            .map(|candidate| (candidate.name.as_str(), candidate.body_offset.is_some()))
            .collect();
        assert_eq!(shapes, [("e", false), ("f", true), ("g", false)]);
    }
}
