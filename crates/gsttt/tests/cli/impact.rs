//! `impact` takes paths or `--changed`, passes what is gone as deleted, follows dependents as far as `--depth` lets it, and with `--name` lists the callers of a name.

use std::fs;
use std::path::{Path, PathBuf};

use gsttt_scenario::scenario;
use serde_json::{Value, json as value};

use super::support::{Sandbox, commit_all, git, is_empty, json, typescript_project, write};

/// The paths of the entries of `list`, a list of answers that carry one.
fn paths(list: &Value) -> Vec<&str> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect()
}

#[scenario("cli", "--changed includes untracked files")]
#[test]
fn changed_includes_untracked_files() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    // Created, and never added to git.
    write(&repo, "app/new.ts", "export const n = 1;\n");

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    assert_eq!(answer["changed"], value!(["app/new.ts"]));
    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert!(
        text.stdout
            .starts_with("impact of app/new.ts, within 1 hop through code\n"),
        "{text:?}"
    );
}

#[scenario("cli", "--changed passes a deleted file as deleted")]
#[test]
fn changed_passes_a_deleted_file_as_deleted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    fs::remove_file(repo.join("lib/util.ts")).unwrap();

    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .contains("\nlib/util.ts is deleted, so it is not traced\n"),
        "{text:?}"
    );
    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    assert_eq!(paths(&answer["deleted"]), ["lib/util.ts"]);
    assert_eq!(answer["changed"], value!([]));
}

#[scenario("cli", "A named path that is not on disk counts as deleted")]
#[test]
fn a_named_path_that_is_not_on_disk_counts_as_deleted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    fs::remove_file(repo.join("lib/util.ts")).unwrap();

    let text = sandbox.run_in(&repo, &["impact", "lib/util.ts"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .contains("\nlib/util.ts is deleted, so it is not traced\n"),
        "{text:?}"
    );
    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert_eq!(paths(&answer["deleted"]), ["lib/util.ts"]);
}

#[scenario("cli", "--changed outside git exits 2")]
#[test]
fn changed_outside_git_exits_2() {
    let sandbox = Sandbox::new();
    typescript_project(sandbox.work.path());
    for args in [
        &["impact", "--changed"][..],
        &["--no-refresh", "impact", "--changed"][..],
        &["--json", "impact", "--changed"][..],
    ] {
        let run = sandbox.run(args);
        assert_eq!(run.code, Some(2), "{args:?}: {run:?}");
        assert_eq!(run.stdout, "", "{args:?}");
        assert!(
            run.stderr.contains("`--changed` needs a git repository"),
            "{args:?}: {run:?}"
        );
        assert!(
            run.stderr.contains("name the files instead"),
            "{args:?}: {run:?}"
        );
    }
    assert!(is_empty(sandbox.cache.path()), "nothing was indexed");
}

#[test]
fn a_named_path_on_disk_is_traced_and_depth_limits_the_walk() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    // lib/util.ts is imported by app/a.ts, which app/c.ts imports.
    write(
        &repo,
        "app/c.ts",
        "import { a } from './a';\nexport const c = a;\n",
    );

    let by_default = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert_eq!(paths(&by_default["affected"]), ["app/a.ts"]);
    assert_eq!(by_default["depth"], 1);

    let everything = json(&sandbox.run_in(
        &repo,
        &["--json", "impact", "--depth", "4294967295", "lib/util.ts"],
    ));
    assert_eq!(paths(&everything["affected"]), ["app/a.ts", "app/c.ts"]);
    assert_eq!(everything["depth"], 4_294_967_295_u32);
}

#[test]
fn names_go_to_the_query_for_every_path_and_a_walk_by_name_takes_one_hop_unless_a_depth_is_given() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let by_name = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "lib/util.ts",
            "app/b.ts",
            "--name",
            "util",
            "--name",
            "other",
        ],
    ));
    assert_eq!(by_name["changed"], value!(["lib/util.ts", "app/b.ts"]));
    assert_eq!(by_name["names"], value!(["util", "other"]));
    assert_eq!(by_name["depth"], 1);

    let deeper = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "lib/util.ts",
            "--name",
            "util",
            "--depth",
            "3",
        ],
    ));
    assert_eq!(deeper["depth"], 3);

    let whole = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert!(whole.get("names").is_none(), "{whole}");
    assert_eq!(whole["depth"], 1);
}

#[scenario("cli", "A blank --name exits 2")]
#[test]
fn a_blank_name_exits_2() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    for args in [
        &["impact", "lib/util.ts", "--name", ""][..],
        &["impact", "lib/util.ts", "--name", "  "][..],
        &[
            "--json",
            "impact",
            "lib/util.ts",
            "--name",
            "util",
            "--name",
            " ",
        ][..],
    ] {
        let run = sandbox.run_in(&repo, args);
        assert_eq!(run.code, Some(2), "{args:?}: {run:?}");
        assert_eq!(run.stdout, "", "{args:?}");
        assert!(
            run.stderr.contains("`--name` needs a name"),
            "{args:?}: {run:?}"
        );
        assert!(run.stderr.contains("--name getHash"), "{args:?}: {run:?}");
    }
    assert!(is_empty(sandbox.cache.path()), "nothing was indexed");
}

#[test]
fn a_blank_name_does_not_hide_the_argument_errors_that_came_first() {
    let sandbox = Sandbox::new();

    for (args, said) in [
        (
            &["impact", "--changed", "--name", ""][..],
            "`--name` needs paths",
        ),
        (
            &["impact", "--base", "main", "lib/util.ts", "--name", ""][..],
            "`--base` needs `--changed`",
        ),
    ] {
        let run = sandbox.run(args);
        assert_eq!(run.code, Some(2), "{args:?}: {run:?}");
        assert!(run.stderr.contains(said), "{args:?}: {run:?}");
    }
}

#[scenario("cli", "--name asks about a name")]
#[test]
fn name_asks_about_a_name() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/a.ts",
        "export function foo() {\n  return 1;\n}\nexport function bar() {\n  return 2;\n}\n",
    );
    // b.ts calls foo in run; c.ts imports the same file and calls only bar.
    write(
        &repo,
        "src/b.ts",
        "import { foo, bar } from './a';\nexport function run() {\n  return foo();\n}\nexport function stop() {\n  return bar();\n}\n",
    );
    write(
        &repo,
        "src/c.ts",
        "import { bar } from './a';\nexport const c = bar();\n",
    );

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.ts", "--name", "foo"]));
    assert_eq!(answer["names"], value!(["foo"]));
    assert_eq!(paths(&answer["affected"]), ["src/b.ts"]);
    assert_eq!(
        answer["affected"][0]["callers"],
        value!([{ "name": "run", "line": 2, "distance": 1 }])
    );

    let text = sandbox.run_in(&repo, &["impact", "src/a.ts", "--name", "foo"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .starts_with("impact of foo in src/a.ts, within 1 hop through code\n"),
        "{text:?}"
    );
    assert!(
        text.stdout
            .contains("\nsrc/b.ts  distance 1  imports foo from src/a.ts; in run (line 2)\n"),
        "{text:?}"
    );

    // Asked about the whole file, c.ts is affected too, so it is the name that leaves it out.
    let whole = json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.ts"]));
    assert_eq!(paths(&whole["affected"]), ["src/b.ts", "src/c.ts"]);
}

#[test]
fn name_lists_the_callers_of_a_name_and_not_the_tests_that_mock_it_or_read_another_member() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/api.ts",
        "export function fetchUser() {\n  return 1;\n}\nexport function other() {\n  return 2;\n}\n",
    );
    write(
        &repo,
        "src/index.ts",
        "export { fetchUser, other } from './api';\n",
    );
    // The only real caller reaches `fetchUser` through the barrel.
    write(
        &repo,
        "src/page.ts",
        "import { fetchUser } from './index';\nexport function render() {\n  return fetchUser();\n}\n",
    );
    // A test that mocks the barrel and imports nothing from it runs what replaces the module.
    write(
        &repo,
        "src/barrel.test.ts",
        "vi.mock('./index', () => ({ fetchUser: vi.fn() }));\n",
    );
    // A partial mock names the module as a type, which does not run.
    write(
        &repo,
        "src/partial.test.ts",
        "vi.mock('./api', async (importOriginal) => ({\n  ...(await importOriginal<typeof import('./api')>()),\n}));\n",
    );
    // A member read from a loaded module takes that member only.
    write(
        &repo,
        "src/reader.test.ts",
        "test('other', async () => {\n  expect((await import('./api')).other()).toBe(2);\n});\n",
    );

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/api.ts",
            "--name",
            "fetchUser",
            "--depth",
            "2",
        ],
    ));

    assert_eq!(paths(&answer["affected"]), ["src/index.ts", "src/page.ts"]);
    assert_eq!(paths(&answer["tests"]), Vec::<&str>::new());
    let page = answer["affected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["path"] == "src/page.ts")
        .unwrap();
    assert_eq!(
        page["callers"],
        value!([{ "name": "render", "line": 2, "distance": 1 }])
    );

    // The member the reader calls is `other`, so it is the one name that reaches it.
    let other = json(&sandbox.run_in(
        &repo,
        &["--json", "impact", "src/api.ts", "--name", "other"],
    ));
    assert_eq!(paths(&other["tests"]), ["src/reader.test.ts"]);
}

/// A repository in which a method of a resolver calls a service on an injected field and serves a GraphQL mutation that a client calls.
fn graphql_through_an_injected_service(sandbox: &Sandbox) -> PathBuf {
    let repo = sandbox.repository();
    write(
        &repo,
        "package.json",
        "{ \"dependencies\": { \"@nestjs/graphql\": \"^12.0.0\", \"graphql-tag\": \"^2.12.0\" } }\n",
    );
    write(
        &repo,
        "src/invoice.service.ts",
        "export class InvoiceService {\n  issue() {}\n}\n",
    );
    // The resolver names the service only in its constructor, so issueInvoice is a caller by the type of the field.
    write(
        &repo,
        "src/invoice.resolver.ts",
        "import { Mutation, Resolver } from '@nestjs/graphql';\nimport { InvoiceService } from './invoice.service';\n\n@Resolver()\nexport class InvoiceResolver {\n  constructor(private readonly invoices: InvoiceService) {}\n\n  @Mutation()\n  issueInvoice() {\n    return this.invoices.issue();\n  }\n}\n",
    );
    write(
        &repo,
        "src/client.ts",
        "import gql from 'graphql-tag';\n\nexport const ISSUE = gql`mutation Issue { issueInvoice }`;\n",
    );
    repo
}

/// What `impact` of `InvoiceService.issue` lists: the resolver, a hop through code from the service, and the client of the mutation its method serves, a hop across a contract further.
fn assert_the_client_of_the_mutation_is_reached(answer: &Value) {
    assert_eq!(
        paths(&answer["affected"]),
        ["src/invoice.resolver.ts", "src/client.ts"],
        "{answer}"
    );
    assert_eq!(answer["affected"][0]["distance"], 1, "{answer}");
    let callers = answer["affected"][0]["callers"].to_string();
    assert!(callers.contains("InvoiceResolver.issueInvoice"), "{answer}");
    assert_eq!(answer["affected"][1]["distance"], 2, "{answer}");
    assert_eq!(
        answer["affected"][1]["reason"],
        value!({ "via": "contract", "role": "calls", "key": "graphql Mutation.issueInvoice" }),
        "{answer}"
    );
}

/// What `impact` of the resolver of the endpoint `a` lists: the clients, as the binary prints them.
fn clients_of_the_resolver(sandbox: &Sandbox, repo: &Path) -> Vec<String> {
    let answer = json(&sandbox.run_in(repo, &["--json", "impact", "api-a/user.resolver.ts"]));
    paths(&answer["affected"])
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_endpoints_of_the_config_scope_a_graphql_change() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "package.json",
        "{ \"dependencies\": { \"@nestjs/graphql\": \"^12.0.0\", \"graphql-tag\": \"^2.12.0\" } }\n",
    );
    for side in ["a", "b"] {
        write(
            &repo,
            &format!("api-{side}/user.resolver.ts"),
            "import { Query, Resolver } from '@nestjs/graphql';\n\n@Resolver()\nexport class UserResolver {\n  @Query()\n  user() {\n    return null;\n  }\n}\n",
        );
        write(
            &repo,
            &format!("web-{side}/client.ts"),
            "import gql from 'graphql-tag';\n\nexport const USER = gql`query User { user }`;\n",
        );
    }

    // Without endpoints the query is served by every resolver and called by every client.
    assert_eq!(
        clients_of_the_resolver(&sandbox, &repo),
        ["web-a/client.ts", "web-b/client.ts"]
    );

    write(
        &repo,
        ".gsttt/config.toml",
        "[[graphql_endpoints]]\nclients = [\"web-a/**\"]\nservers = [\"api-a/**\"]\n\n[[graphql_endpoints]]\nclients = [\"web-b/**\"]\nservers = [\"api-b/**\"]\n",
    );
    // The first run after the edit builds the index again and stores it with the endpoints.
    assert_eq!(
        clients_of_the_resolver(&sandbox, &repo),
        ["web-a/client.ts"]
    );
    let stored = json(&sandbox.run_in(&repo, &["--json", "index"]));
    assert_eq!(stored["unchanged"], true, "{stored}");
    // The next one reads that snapshot, and the endpoints in it scope the change as before.
    assert_eq!(
        clients_of_the_resolver(&sandbox, &repo),
        ["web-a/client.ts"]
    );
    let still = json(&sandbox.run_in(&repo, &["--json", "index"]));
    assert_eq!(still["unchanged"], true, "{still}");
}

#[test]
fn name_crosses_the_contract_from_a_method_of_an_injected_service() {
    let sandbox = Sandbox::new();
    let repo = graphql_through_an_injected_service(&sandbox);

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/invoice.service.ts",
            "--name",
            "InvoiceService.issue",
            "--depth",
            "2",
        ],
    ));

    assert_the_client_of_the_mutation_is_reached(&answer);
}

#[test]
fn name_crosses_the_contract_from_a_method_of_an_injected_service_without_a_depth() {
    let sandbox = Sandbox::new();
    let repo = graphql_through_an_injected_service(&sandbox);

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/invoice.service.ts",
            "--name",
            "InvoiceService.issue",
        ],
    ));

    // The hop across the contract is no hop through code, so the depth of one the walk takes by name is enough.
    assert_the_client_of_the_mutation_is_reached(&answer);
    assert_eq!(answer["depth"], 1, "{answer}");
}

#[test]
fn name_crosses_only_the_contract_of_the_method_that_calls_the_member() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "package.json",
        "{ \"dependencies\": { \"@nestjs/graphql\": \"^12.0.0\", \"graphql-tag\": \"^2.12.0\" } }\n",
    );
    write(
        &repo,
        "src/invoice.service.ts",
        "export class InvoiceService {\n  issue() {}\n  cancel() {}\n}\n",
    );
    // Both resolver methods read the same injected field, and each serves a mutation of its own.
    write(
        &repo,
        "src/invoice.resolver.ts",
        "import { Mutation, Resolver } from '@nestjs/graphql';\nimport { InvoiceService } from './invoice.service';\n\n@Resolver()\nexport class InvoiceResolver {\n  constructor(private readonly invoices: InvoiceService) {}\n\n  @Mutation()\n  issueInvoice() {\n    return this.invoices.issue();\n  }\n\n  @Mutation()\n  cancelInvoice() {\n    return this.invoices.cancel();\n  }\n}\n",
    );
    write(
        &repo,
        "src/issue.client.ts",
        "import gql from 'graphql-tag';\n\nexport const ISSUE = gql`mutation Issue { issueInvoice }`;\n",
    );
    write(
        &repo,
        "src/cancel.client.ts",
        "import gql from 'graphql-tag';\n\nexport const CANCEL = gql`mutation Cancel { cancelInvoice }`;\n",
    );

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/invoice.service.ts",
            "--name",
            "InvoiceService.issue",
            "--depth",
            "2",
        ],
    ));
    assert_eq!(
        paths(&answer["affected"]),
        ["src/invoice.resolver.ts", "src/issue.client.ts"],
        "{answer}"
    );
    let callers = answer["affected"][0]["callers"].to_string();
    assert!(callers.contains("InvoiceResolver.issueInvoice"), "{answer}");
    assert!(
        !callers.contains("InvoiceResolver.cancelInvoice"),
        "{answer}"
    );
}

#[scenario("cli", "--no-tests leaves the tests out")]
#[test]
fn no_tests_leaves_the_tests_out_and_counts_them() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "src/a.ts", "export const a = 1;\n");
    write(
        &repo,
        "src/b.ts",
        "import { a } from './a';\nexport const b = a;\n",
    );
    for test in ["a", "b"] {
        write(
            &repo,
            &format!("src/{test}.test.ts"),
            "import { a } from './a';\ntest('a', () => expect(a).toBe(1));\n",
        );
    }

    let text = sandbox.run_in(&repo, &["impact", "src/a.ts", "--no-tests"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .contains("\nsrc/b.ts  distance 1  imports a from src/a.ts\n"),
        "{text:?}"
    );
    assert!(
        text.stdout.contains("\n2 test files left out\n"),
        "{text:?}"
    );
    assert!(!text.stdout.contains(".test.ts"), "{text:?}");

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.ts", "--no-tests"]));
    assert_eq!(paths(&answer["affected"]), ["src/b.ts"], "{answer}");
    assert_eq!(answer["tests"], value!([]), "{answer}");
    assert_eq!(answer["tests_left_out"], 2, "{answer}");

    // The tests are listed unless the question leaves them out.
    let listed = json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.ts"]));
    assert_eq!(
        paths(&listed["tests"]),
        ["src/a.test.ts", "src/b.test.ts"],
        "{listed}"
    );
    assert!(listed.get("tests_left_out").is_none(), "{listed}");
}

#[test]
fn name_asks_about_a_member_of_a_class() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    // stop calls start through this; main gets an instance of the class, so what it calls is found by the class it names.
    write(
        &repo,
        "src/runner.ts",
        "export class Runner {\n  start() {}\n  stop() {\n    this.start();\n  }\n}\n",
    );
    write(
        &repo,
        "src/app.ts",
        "import { Runner } from './runner';\nexport function main() {\n  new Runner().start();\n}\n",
    );

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/runner.ts",
            "--name",
            "Runner.start",
        ],
    ));
    assert_eq!(answer["names"], value!(["Runner.start"]));
    assert_eq!(paths(&answer["affected"]), ["src/app.ts", "src/runner.ts"]);
    assert_eq!(
        answer["affected"][0]["reason"],
        value!({ "via": "imports", "file": "src/runner.ts", "names": ["Runner"] })
    );
    assert_eq!(
        answer["affected"][0]["callers"],
        value!([{ "name": "main", "line": 2, "distance": 1 }])
    );
    assert_eq!(
        answer["affected"][1]["reason"],
        value!({ "via": "declares", "names": ["Runner.start"] })
    );
    assert_eq!(
        answer["affected"][1]["callers"],
        value!([{ "name": "Runner.stop", "line": 3, "distance": 1 }])
    );
    assert!(answer.get("notes").is_none_or(Value::is_null), "{answer}");

    let text = sandbox.run_in(
        &repo,
        &["impact", "src/runner.ts", "--name", "Runner.start"],
    );
    assert_eq!(text.code, Some(0), "{text:?}");
    assert!(
        text.stdout
            .starts_with("impact of Runner.start in src/runner.ts, within 1 hop through code\n"),
        "{text:?}"
    );
    assert!(
        text.stdout.contains(
            "\nsrc/app.ts  distance 1  imports Runner from src/runner.ts; in main (line 2)\n"
        ),
        "{text:?}"
    );
    assert!(
        text.stdout.contains(
            "\nsrc/runner.ts  distance 1  declares Runner.start; in Runner.stop (line 3)\n"
        ),
        "{text:?}"
    );
    assert!(!text.stdout.contains("\nnote: "), "{text:?}");
}

/// A module that only lists the class in an array holds it without calling a member, so it is left out of the callers of a method and counted in a note, while the service that calls the method through a typed field is listed.
#[test]
fn a_file_that_only_holds_a_class_is_counted_and_not_listed_in_typescript() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/service.ts",
        "export class Service {\n  place() {}\n}\n",
    );
    write(
        &repo,
        "src/module.ts",
        "import { Service } from './service';\n\nexport const providers = [Service];\n",
    );
    write(
        &repo,
        "src/caller.ts",
        "import { Service } from './service';\n\nexport class Caller {\n  constructor(private readonly service: Service) {}\n\n  run() {\n    this.service.place();\n  }\n}\n",
    );

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/service.ts",
            "--name",
            "Service.place",
        ],
    ));
    assert_eq!(paths(&answer["affected"]), ["src/caller.ts"], "{answer}");
    assert_eq!(
        answer["notes"],
        value!([
            "1 file holds Service and reads no place through it; ask about Service to list them"
        ])
    );
}

/// The real connector reading `CommonJS`: a file that lists its exports, a require at the top level that binds a local and is used for one name, and a require inside a function that reads none of the names asked about, which is counted in a note and not listed.
#[test]
fn a_file_that_loads_a_commonjs_module_in_a_function_and_reads_no_name_is_counted() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/lib.js",
        "function helper() {}\n\nfunction other() {}\n\nmodule.exports = { helper, other };\n",
    );
    write(
        &repo,
        "src/top.js",
        "const lib = require('./lib');\n\nfunction run() {\n  return lib.helper();\n}\n\nfunction stop() {\n  return lib.other();\n}\n\nmodule.exports = { run, stop };\n",
    );
    write(
        &repo,
        "src/nested.js",
        "function load() {\n  const lib = require('./lib');\n  return lib.other();\n}\n\nmodule.exports = { load };\n",
    );

    let answer = json(&sandbox.run_in(
        &repo,
        &["--json", "impact", "src/lib.js", "--name", "helper"],
    ));
    assert_eq!(
        paths(&answer["affected"]),
        ["src/lib.js", "src/top.js"],
        "{answer}"
    );
    // The walk also stops at the depth, which is another note.
    assert_eq!(
        answer["notes"][0].as_str(),
        Some(
            "1 file loads this module and reads none of the names asked about; ask about the file to list it"
        )
    );
}

/// The real connector reading `CommonJS`: a file that writes its exports in place declares them, so a question about one name lists the file that reads it through a require and not the file that reads another.
#[scenario("queries", "A name written in place lists the files that read it")]
#[test]
fn a_name_a_commonjs_file_writes_in_place_lists_the_files_that_read_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/lib.js",
        "exports.run = function () {\n  return 1;\n};\n\nexports.stop = function () {\n  return 2;\n};\n",
    );
    write(
        &repo,
        "src/runs.js",
        "const lib = require('./lib');\n\nmodule.exports = lib.run();\n",
    );
    write(
        &repo,
        "src/stops.js",
        "const lib = require('./lib');\n\nmodule.exports = lib.stop();\n",
    );

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "src/lib.js", "--name", "run"]));
    assert_eq!(paths(&answer["affected"]), ["src/runs.js"], "{answer}");
}

/// The real connector reading a destructuring: a file that takes `a` out of a module is a caller of `a`, and not of the other names of the module.
#[scenario("queries", "A destructured name lists the files that take it")]
#[test]
fn a_destructured_name_lists_the_files_that_take_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "lib.js",
        "function a() {}\n\nfunction other() {}\n\nmodule.exports = { a, other };\n",
    );
    write(
        &repo,
        "taker.js",
        "const lib = require('./lib');\n\nfunction run() {\n  const { a } = lib;\n  return a();\n}\n\nmodule.exports = { run };\n",
    );

    let other = json(&sandbox.run_in(&repo, &["--json", "impact", "lib.js", "--name", "other"]));
    assert_eq!(paths(&other["affected"]), ["lib.js"], "{other}");
    let taken = json(&sandbox.run_in(&repo, &["--json", "impact", "lib.js", "--name", "a"]));
    assert_eq!(paths(&taken["affected"]), ["lib.js", "taker.js"], "{taken}");
}

/// The real connector reading `CommonJS`: a computed key written as a statement of the top level leaves the exports known, so a class a file only uses is not passed on to the files that require it, and what the computed key hands out is still reached.
#[scenario("queries", "A computed export key does not pass an imported name on")]
#[test]
fn a_computed_export_key_does_not_pass_an_imported_name_on() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/client.js",
        "class Client {}\n\nmodule.exports = Client;\n",
    );
    write(
        &repo,
        "src/a.js",
        "const Client = require('./client');\n\nfunction helper() {}\n\nfunction run() {\n  return new Client();\n}\n\nmodule.exports = run;\nmodule.exports[Symbol.for('x')] = { helper };\n",
    );
    write(
        &repo,
        "src/b.js",
        "const run = require('./a');\n\nmodule.exports = run;\n",
    );
    write(
        &repo,
        "src/c.js",
        "const helper = require('./a')[Symbol.for('x')];\n\nmodule.exports = helper;\n",
    );

    let class = json(&sandbox.run_in(
        &repo,
        &["--json", "impact", "src/client.js", "--name", "Client"],
    ));
    assert_eq!(
        paths(&class["affected"]),
        ["src/a.js", "src/client.js"],
        "{class}"
    );
    let helper =
        json(&sandbox.run_in(&repo, &["--json", "impact", "src/a.js", "--name", "helper"]));
    assert_eq!(
        paths(&helper["affected"]),
        ["src/a.js", "src/c.js"],
        "{helper}"
    );
}

#[test]
fn a_path_the_index_does_not_hold_does_not_fail_the_others() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "notes.txt", "no connector reads this\n");

    let run = sandbox.run_in(&repo, &["--json", "impact", "notes.txt", "lib/util.ts"]);
    let answer = json(&run);
    assert_eq!(answer["unknown"], value!(["notes.txt"]));
    assert_eq!(paths(&answer["affected"]), ["app/a.ts"]);
}

#[test]
fn nothing_changed_is_an_answer() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let run = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert_eq!(run.code, Some(0), "{run:?}");
    assert!(
        run.stdout
            .starts_with("impact of no files, within 1 hop through code\n"),
        "{run:?}"
    );
}

#[test]
fn base_compares_with_the_best_common_ancestor_of_head_and_the_revision() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    git(&repo, &["branch", "base"]);
    write(&repo, "app/c.ts", "export const c = 3;\n");
    commit_all(&repo);

    let from_head = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    assert_eq!(from_head["changed"], value!([]));
    let from_base =
        json(&sandbox.run_in(&repo, &["--json", "impact", "--changed", "--base", "base"]));
    assert_eq!(from_base["changed"], value!(["app/c.ts"]));
}

#[test]
fn a_base_that_is_no_revision_exits_2() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();

    let run = sandbox.run_in(&repo, &["impact", "--changed", "--base", "no-such-branch"]);
    assert_eq!(run.code, Some(2), "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    assert!(
        run.stderr
            .contains("`no-such-branch` is not a revision of this repository"),
        "{run:?}"
    );
    assert!(run.stderr.contains("--base"), "{run:?}");
}

/// A service with two members, a caller of each through a new instance, and a function that only takes the service as a parameter.
fn write_order_service(repo: &Path) {
    write(
        repo,
        "src/service.ts",
        "export class OrderService {\n  place(id: string) {}\n  cancel(id: string) {}\n}\n",
    );
    write(
        repo,
        "src/refunds.ts",
        "import { OrderService } from './service';\n\nexport function refund(id: string) {\n  new OrderService().cancel(id);\n}\n",
    );
    write(
        repo,
        "src/checkout.ts",
        "import { OrderService } from './service';\n\nexport function pay(id: string) {\n  new OrderService().place(id);\n}\n",
    );
    write(
        repo,
        "src/keep.ts",
        "import { OrderService } from './service';\n\nexport function keep(service: OrderService) {}\n",
    );
}

#[scenario(
    "queries",
    "A caller of another member through a new instance is not listed"
)]
#[test]
fn a_caller_of_another_member_through_a_new_instance_is_not_listed() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_order_service(&repo);

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/service.ts",
            "--name",
            "OrderService.place",
        ],
    ));
    assert!(
        paths(&answer["affected"]).contains(&"src/checkout.ts"),
        "{answer}"
    );
    let affected = answer["affected"].to_string();
    assert!(affected.contains("pay"), "{answer}");
    assert!(!affected.contains("src/refunds.ts"), "{answer}");
}

#[scenario("queries", "Asked about the class, every caller of a member is listed")]
#[test]
fn asked_about_the_class_every_caller_of_a_member_is_listed() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_order_service(&repo);

    let answer = json(&sandbox.run_in(
        &repo,
        &[
            "--json",
            "impact",
            "src/service.ts",
            "--name",
            "OrderService",
        ],
    ));
    let listed = paths(&answer["affected"]);
    for path in ["src/checkout.ts", "src/refunds.ts", "src/keep.ts"] {
        assert!(listed.contains(&path), "{path}: {answer}");
    }
}

/// The `affected` entries of the impact of `name` declared in `src/svc.ts`, as each path with the names of its callers.
fn callers_of(sandbox: &Sandbox, repo: &Path, name: &str) -> Vec<(String, Vec<String>)> {
    callers_in(sandbox, repo, "src/svc.ts", name)
}

/// The `affected` entries of the impact of `name` declared in `file`, as each path with the names of its callers.
fn callers_in(
    sandbox: &Sandbox,
    repo: &Path,
    file: &str,
    name: &str,
) -> Vec<(String, Vec<String>)> {
    let answer = json(&sandbox.run_in(repo, &["--json", "impact", file, "--name", name]));
    answer["affected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let callers = entry["callers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|caller| caller["name"].as_str().unwrap().to_owned())
                .collect();
            (entry["path"].as_str().unwrap().to_owned(), callers)
        })
        .collect()
}

/// The real connector reading the namespace scenarios of the core tests: what `uses` holds for `lib.name`, `consume(lib)`, `lib[key]()` and `const { helper } = lib`.
#[test]
fn a_namespace_import_is_a_caller_only_of_the_names_it_uses_in_typescript() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/svc.ts",
        "export function helper() {}\nexport function other() {}\nexport interface Foo {}\nexport function Comp() {}\nexport class Service {\n  place() {}\n  cancel() {}\n  static make() {\n    return new Service();\n  }\n}\n",
    );
    let namespace = "import * as lib from './svc';\n\n";
    write(
        &repo,
        "src/a.ts",
        &format!(
            "{namespace}export function viaHelper() {{\n  lib.helper();\n}}\n\nexport function viaOther() {{\n  lib.other();\n}}\n"
        ),
    );
    write(
        &repo,
        "src/b.ts",
        &format!("{namespace}export function onlyOther() {{\n  lib.other();\n}}\n"),
    );
    write(
        &repo,
        "src/c.ts",
        &format!(
            "{namespace}declare function consume(value: unknown): void;\nexport function pass() {{\n  consume(lib);\n}}\n"
        ),
    );
    write(
        &repo,
        "src/d.ts",
        &format!(
            "{namespace}export function pick(key: 'helper' | 'other') {{\n  lib[key]();\n}}\n"
        ),
    );
    write(
        &repo,
        "src/e.ts",
        &format!(
            "{namespace}export function spread() {{\n  const {{ helper }} = lib;\n  helper();\n}}\n"
        ),
    );
    write(
        &repo,
        "src/f.ts",
        &format!(
            "{namespace}export function build() {{\n  return new lib.Service().place();\n}}\n\nexport function make() {{\n  return lib.Service.make();\n}}\n"
        ),
    );

    write(
        &repo,
        "src/g.ts",
        &format!("{namespace}export function typed(x: lib.Foo) {{}}\n"),
    );
    write(
        &repo,
        "src/k.ts",
        &format!(
            "{namespace}declare function consume(value: unknown): void;\nexport function escapes(x: lib.Foo) {{\n  consume(x);\n  return x;\n}}\n"
        ),
    );
    write(
        &repo,
        "src/l.ts",
        "import type * as T from './svc';\n\nexport function escapesType(x: T.Foo) {\n  return x;\n}\n",
    );
    write(
        &repo,
        "src/h.tsx",
        &format!("{namespace}export function jsx() {{\n  return <lib.Comp />;\n}}\n"),
    );
    write(
        &repo,
        "src/i.ts",
        &format!("{namespace}export function queried(x: typeof lib.helper) {{}}\n"),
    );
    write(
        &repo,
        "src/j.ts",
        "import type * as T from './svc';\n\nexport function onlyType(x: T.Foo) {}\n",
    );

    let helper = callers_of(&sandbox, &repo, "helper");
    let listed = |path: &str| helper.iter().find(|(listed, _)| listed == path);
    let owned = |names: &[&str]| {
        names
            .iter()
            .map(|&name| name.to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        listed("src/a.ts").map(|(_, callers)| callers.clone()),
        Some(owned(&["viaHelper"])),
        "{helper:?}"
    );
    assert_eq!(listed("src/b.ts"), None, "{helper:?}");
    for (path, name) in [
        ("src/c.ts", "pass"),
        ("src/d.ts", "pick"),
        ("src/e.ts", "spread"),
    ] {
        assert_eq!(
            listed(path).map(|(_, callers)| callers.clone()),
            Some(owned(&[name])),
            "{path}: {helper:?}"
        );
    }
    assert_eq!(listed("src/f.ts"), None, "{helper:?}");
    // A qualified type name and a JSX member are uses of the name they write, as a read of `lib.name` is.
    assert_eq!(listed("src/g.ts"), None, "{helper:?}");
    assert_eq!(listed("src/h.tsx"), None, "{helper:?}");
    assert_eq!(listed("src/j.ts"), None, "{helper:?}");
    assert_eq!(listed("src/k.ts"), None, "{helper:?}");
    assert_eq!(listed("src/l.ts"), None, "{helper:?}");
    assert_eq!(
        listed("src/i.ts").map(|(_, callers)| callers.clone()),
        Some(owned(&["queried"])),
        "{helper:?}"
    );

    let foo = callers_of(&sandbox, &repo, "Foo");
    let foo_callers = |path: &str| {
        foo.iter()
            .find(|(listed, _)| listed == path)
            .map(|(_, callers)| callers.clone())
    };
    assert_eq!(foo_callers("src/g.ts"), Some(owned(&["typed"])), "{foo:?}");
    assert_eq!(
        foo_callers("src/j.ts"),
        Some(owned(&["onlyType"])),
        "{foo:?}"
    );
    assert_eq!(
        foo_callers("src/k.ts"),
        Some(owned(&["escapes"])),
        "{foo:?}"
    );
    assert_eq!(
        foo_callers("src/l.ts"),
        Some(owned(&["escapesType"])),
        "{foo:?}"
    );
    assert_eq!(foo_callers("src/i.ts"), None, "{foo:?}");

    let comp = callers_of(&sandbox, &repo, "Comp");
    let comp_callers = comp
        .iter()
        .find(|(path, _)| path == "src/h.tsx")
        .map(|(_, callers)| callers.clone());
    assert_eq!(comp_callers, Some(owned(&["jsx"])), "{comp:?}");

    let place = callers_of(&sandbox, &repo, "Service.place");
    let callers = place
        .iter()
        .find(|(path, _)| path == "src/f.ts")
        .map(|(_, callers)| callers.clone());
    assert_eq!(
        callers,
        Some(owned(&["build"])),
        "`make` reads `lib.Service.make`, a member other than `place`: {place:?}"
    );
}

const FORMAT: &str = "export function formatUser(name: string): string {\n  return normalize(name).trim();\n}\n\nfunction normalize(text: string): string {\n  return text.toLowerCase();\n}\n\nexport function formatId(id: number): string {\n  return String(id);\n}\n";

const PAGE: &str = "import { formatUser } from '../format';\n\nexport function renderPage(name: string): string {\n  return formatUser(name);\n}\n";

const AUDIT: &str = "import { formatId } from '../format';\n\nexport function audit(id: number): string {\n  return formatId(id);\n}\n";

/// A repository in which `src/format.ts` has two exported functions and a private helper of one of them. `src/users` imports `formatUser` and `src/admin` imports `formatId`, each from a file and from a test.
fn formatting(sandbox: &Sandbox) -> PathBuf {
    let repo = sandbox.repository();
    write(&repo, "src/format.ts", FORMAT);
    write(&repo, "src/users/page.ts", PAGE);
    write(
        &repo,
        "src/users/page.test.ts",
        &PAGE.replace("renderPage", "pageTest"),
    );
    write(&repo, "src/admin/audit.ts", AUDIT);
    write(
        &repo,
        "src/admin/audit.test.ts",
        &AUDIT.replace("audit", "auditTest"),
    );
    commit_all(&repo);
    repo
}

/// The `--json` answer of `impact --changed` with `more` arguments.
fn changed(sandbox: &Sandbox, repo: &Path, more: &[&str]) -> Value {
    let mut args = vec!["--json", "impact", "--changed"];
    args.extend_from_slice(more);
    let run = sandbox.run_in(repo, &args);
    assert_eq!(run.code, Some(0), "{run:?}");
    json(&run)
}

/// The files listed as affected and as tests, sorted.
fn reached(answer: &Value) -> Vec<&str> {
    let mut listed = paths(&answer["affected"]);
    listed.extend(paths(&answer["tests"]));
    listed.sort_unstable();
    listed
}

const USERS: [&str; 2] = ["src/users/page.test.ts", "src/users/page.ts"];

const EVERYONE: [&str; 4] = [
    "src/admin/audit.test.ts",
    "src/admin/audit.ts",
    "src/users/page.test.ts",
    "src/users/page.ts",
];

#[scenario("cli", "--changed walks the declarations a patch changes")]
#[test]
fn changed_walks_the_declarations_a_patch_changes() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace(
            "normalize(name).trim()",
            "normalize(name).trim().slice(0, 40)",
        ),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), USERS);
    assert_eq!(
        answer["parts"],
        value!([{ "file": "src/format.ts", "names": ["formatUser"] }])
    );
    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert!(
        text.stdout
            .starts_with("impact of formatUser in src/format.ts, within 1 hop through code\n"),
        "{text:?}"
    );
}

#[scenario(
    "cli",
    "--changed follows a changed private helper to the declarations that use it"
)]
#[test]
fn changed_follows_a_changed_private_helper_to_the_declarations_that_use_it() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace("text.toLowerCase()", "text.toUpperCase()"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), USERS);
    let names = answer["parts"][0]["names"].as_array().unwrap();
    assert!(names.contains(&value!("normalize")), "{answer}");
}

#[scenario("cli", "--changed does not trace a file where no declaration changed")]
#[test]
fn changed_does_not_trace_a_file_where_no_declaration_changed() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    let reindented = FORMAT
        .replace("  return", "      return")
        .replace("\n\n", "\n\n\n");
    write(
        &repo,
        "src/format.ts",
        &format!("// Formats names and ids.\n{reindented}"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(answer["changed"], value!(["src/format.ts"]));
    assert_eq!(reached(&answer), Vec::<&str>::new());
    assert_eq!(
        answer["parts"],
        value!([{ "file": "src/format.ts", "names": [] }])
    );
    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert!(
        text.stdout
            .contains("\nsrc/format.ts changes no declaration, so it is not traced\n"),
        "{text:?}"
    );
}

#[scenario(
    "cli",
    "--changed reads a multi-line import that gains a name only one declaration uses"
)]
#[test]
fn changed_reads_a_multi_line_import_that_gains_a_name_only_one_declaration_uses() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/text.ts",
        "export const shout = (text: string) => text;\n",
    );
    let view = "import {\n  formatUser,\n} from './format';\n\nexport function show(name: string): string {\n  return formatUser(name);\n}\n\nexport function plain(): string {\n  return 'plain';\n}\n";
    write(&repo, "src/view.ts", view);
    write(
        &repo,
        "src/users/shown.ts",
        "import { show } from '../view';\n\nexport const shown = show('a');\n",
    );
    write(
        &repo,
        "src/users/plain.ts",
        "import { plain } from '../view';\n\nexport const flat = plain();\n",
    );
    commit_all(&repo);
    write(
        &repo,
        "src/view.ts",
        &view
            .replace(
                "  formatUser,\n}",
                "  formatUser,\n} from './format';\nimport {\n  shout,\n}",
            )
            .replace("return formatUser(name)", "return shout(formatUser(name))"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), ["src/users/shown.ts"]);
    assert_eq!(answer["parts"][0]["names"], value!(["show"]));
}

#[scenario(
    "cli",
    "--changed follows a re-pointed import to the declarations that use it"
)]
#[test]
fn changed_follows_a_re_pointed_import_to_the_declarations_that_use_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "src/a.ts", "export const clock = () => 1;\n");
    write(&repo, "src/b.ts", "export const clock = () => 2;\n");
    write(
        &repo,
        "src/user.ts",
        "import { clock } from './a';\n\nexport function now() {\n  return clock();\n}\n\nexport function other() {\n  return 0;\n}\n",
    );
    write(
        &repo,
        "src/use-now.ts",
        "import { now } from './user';\n\nexport const t = now();\n",
    );
    write(
        &repo,
        "src/use-other.ts",
        "import { other } from './user';\n\nexport const o = other();\n",
    );
    commit_all(&repo);
    let user = fs::read_to_string(repo.join("src/user.ts")).unwrap();
    write(&repo, "src/user.ts", &user.replace("'./a'", "'./b'"));

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), ["src/use-now.ts"]);
    assert_eq!(answer["parts"][0]["names"], value!(["now"]));
}

/// `src/format.ts` without `formatId`.
fn without_format_id() -> &'static str {
    FORMAT.split("export function formatId").next().unwrap()
}

#[scenario("cli", "--changed walks the importers of a removed declaration")]
#[test]
fn changed_walks_the_importers_of_a_removed_declaration() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(&repo, "src/format.ts", without_format_id());

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(
        reached(&answer),
        ["src/admin/audit.test.ts", "src/admin/audit.ts"],
        "{answer}"
    );
    assert_eq!(
        answer["parts"],
        value!([{ "file": "src/format.ts", "names": [], "removed": ["formatId"] }])
    );
    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert!(
        text.stdout
            .starts_with("impact of removed formatId in src/format.ts, within 1 hop"),
        "{text:?}"
    );
}

#[scenario(
    "cli",
    "--changed lists no importer of a removed declaration nobody imports"
)]
#[test]
fn changed_lists_no_importer_of_a_removed_declaration_nobody_imports() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "src/format.ts", FORMAT);
    write(&repo, "src/users/page.ts", PAGE);
    commit_all(&repo);
    write(&repo, "src/format.ts", without_format_id());

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), Vec::<&str>::new(), "{answer}");
    assert_eq!(answer["changed"], value!(["src/format.ts"]));
}

#[scenario("cli", "--changed walks a renamed declaration by its old name")]
#[test]
fn changed_walks_a_renamed_declaration_by_its_old_name() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/keys.ts",
        "import { formatKey } from './format';\n\nexport const key = formatKey(1);\n",
    );
    commit_all(&repo);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace("formatId", "formatKey"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(
        reached(&answer),
        [
            "src/admin/audit.test.ts",
            "src/admin/audit.ts",
            "src/keys.ts"
        ],
        "{answer}"
    );
    assert_eq!(
        answer["parts"],
        value!([{
            "file": "src/format.ts",
            "names": ["formatKey"],
            "removed": ["formatId"]
        }])
    );
}

#[scenario(
    "cli",
    "--changed lists only the changed importers of a healthy rename"
)]
#[test]
fn changed_lists_only_the_changed_importers_of_a_healthy_rename() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace("formatId", "formatKey"),
    );
    write(
        &repo,
        "src/admin/audit.ts",
        &AUDIT.replace("formatId", "formatKey"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(
        answer["changed"],
        value!(["src/admin/audit.ts", "src/format.ts"])
    );
    let listed = reached(&answer);
    assert!(
        !listed.iter().any(|path| path.starts_with("src/users/")),
        "{answer}"
    );
    assert_eq!(listed, ["src/admin/audit.test.ts"], "{answer}");
}

#[scenario(
    "cli",
    "--changed walks the importers of a declaration that stops being exported"
)]
#[test]
fn changed_walks_the_importers_of_a_declaration_that_stops_being_exported() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace("export function formatId", "function formatId"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(
        reached(&answer),
        ["src/admin/audit.test.ts", "src/admin/audit.ts"],
        "{answer}"
    );
    assert_eq!(answer["parts"][0]["removed"], value!(["formatId"]));
}

#[scenario("cli", "--changed walks the importers of a removed default export")]
#[test]
fn changed_walks_the_importers_of_a_removed_default_export() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let base = "export default function greet(): string {\n  return 'hi';\n}\n\nexport function other(): number {\n  return 1;\n}\n";
    write(&repo, "src/def.ts", base);
    write(
        &repo,
        "src/hello.ts",
        "import greet from './def';\n\nexport const hello = greet();\n",
    );
    write(
        &repo,
        "src/more.ts",
        "import { other } from './def';\n\nexport const more = other();\n",
    );
    commit_all(&repo);
    write(
        &repo,
        "src/def.ts",
        "export function other(): number {\n  return 1;\n}\n",
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), ["src/hello.ts"], "{answer}");
}

#[scenario("cli", "--changed walks the callers of a removed member")]
#[test]
fn changed_walks_the_callers_of_a_removed_member() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_order_service(&repo);
    commit_all(&repo);
    write(
        &repo,
        "src/service.ts",
        "export class OrderService {\n  place(id: string) {}\n}\n",
    );

    let answer = changed(&sandbox, &repo, &[]);

    let listed = reached(&answer);
    assert!(listed.contains(&"src/refunds.ts"), "{answer}");
    assert!(!listed.contains(&"src/checkout.ts"), "{answer}");
    assert_eq!(
        answer["parts"][0]["removed"],
        value!(["OrderService.cancel"])
    );
}

#[scenario("cli", "--changed walks a removed declaration through a barrel")]
#[test]
fn changed_walks_a_removed_declaration_through_a_barrel() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "src/format.ts", FORMAT);
    write(&repo, "src/star.ts", "export * from './format';\n");
    write(
        &repo,
        "src/named.ts",
        "export { formatId, formatUser } from './format';\n",
    );
    for (barrel, name) in [
        ("star", "formatId"),
        ("star", "formatUser"),
        ("named", "formatId"),
        ("named", "formatUser"),
    ] {
        write(
            &repo,
            &format!("src/use-{barrel}-{name}.ts"),
            &format!("import {{ {name} }} from './{barrel}';\n\nexport const x = {name}(1);\n"),
        );
    }
    commit_all(&repo);
    write(&repo, "src/format.ts", without_format_id());

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(
        reached(&answer),
        [
            "src/named.ts",
            "src/use-named-formatId.ts",
            "src/use-star-formatId.ts"
        ],
        "{answer}"
    );
}

#[scenario(
    "cli",
    "--changed walks a file whole when a declaration moves and another is removed"
)]
#[test]
fn changed_walks_a_file_whole_when_a_declaration_moves_and_another_is_removed() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    let (user, rest) = without_format_id()
        .split_once("\nfunction normalize")
        .unwrap();
    write(
        &repo,
        "src/format.ts",
        &format!("function normalize{rest}\n{user}"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), EVERYONE, "{answer}");
    assert!(answer.get("parts").is_none(), "{answer}");
}

#[scenario(
    "cli",
    "--changed lists no importer of a new declaration nobody imports"
)]
#[test]
fn changed_lists_no_importer_of_a_new_declaration_nobody_imports() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/format.ts",
        &format!(
            "{FORMAT}\nexport function formatDate(date: Date): string {{\n  return date.toISOString();\n}}\n"
        ),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), Vec::<&str>::new());
    assert_eq!(
        answer["parts"],
        value!([{ "file": "src/format.ts", "names": ["formatDate"] }])
    );
}

#[scenario(
    "cli",
    "--changed walks a file whole when a side-effect import or a top-level statement changes"
)]
#[test]
fn changed_walks_a_file_whole_when_a_side_effect_import_or_a_top_level_statement_changes() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(&repo, "src/polyfill.ts", "export {};\n");
    write(&repo, "src/polyfill-next.ts", "export {};\n");
    write(
        &repo,
        "src/format.ts",
        &format!("import './polyfill';\n{FORMAT}"),
    );
    commit_all(&repo);
    let committed = fs::read_to_string(repo.join("src/format.ts")).unwrap();
    for edited in [
        committed.replace("'./polyfill'", "'./polyfill-next'"),
        format!("{committed}\nconsole.log('loaded');\n"),
    ] {
        write(&repo, "src/format.ts", &edited);

        let answer = changed(&sandbox, &repo, &[]);

        assert_eq!(reached(&answer), EVERYONE, "{answer}");
        assert!(answer.get("parts").is_none(), "{answer}");
        git(&repo, &["checkout", "--", "."]);
    }
}

#[scenario(
    "cli",
    "--changed walks a changed file whole when its language records no declarations"
)]
#[test]
fn changed_walks_a_changed_file_whole_when_its_language_records_no_declarations() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(&repo, "tools/job.py", "def run():\n    return 1\n");
    write(&repo, "tools/main.py", "from job import run\n\nrun()\n");
    commit_all(&repo);
    write(&repo, "tools/job.py", "def run():\n    return 2\n");
    write(&repo, "src/fresh.ts", "export const fresh = 1;\n");

    let answer = changed(&sandbox, &repo, &[]);

    assert!(answer.get("parts").is_none(), "{answer}");
    let text = sandbox.run_in(&repo, &["impact", "--changed"]);
    assert!(
        text.stdout
            .starts_with("impact of src/fresh.ts and tools/job.py, within 1 hop through code\n"),
        "{text:?}"
    );
}

#[scenario("cli", "--changed with --base compares with the merge base")]
#[test]
fn changed_with_base_compares_with_the_merge_base() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    git(&repo, &["branch", "-M", "main"]);
    git(&repo, &["checkout", "--quiet", "-b", "topic"]);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace("normalize(name).trim()", "normalize(name)"),
    );
    commit_all(&repo);
    let topic = fs::read_to_string(repo.join("src/format.ts")).unwrap();
    write(
        &repo,
        "src/format.ts",
        &topic.replace("String(id)", "`${id}`"),
    );

    let from_head = changed(&sandbox, &repo, &[]);
    assert_eq!(from_head["parts"][0]["names"], value!(["formatId"]));
    assert_eq!(
        reached(&from_head),
        ["src/admin/audit.test.ts", "src/admin/audit.ts"]
    );

    let from_base = changed(&sandbox, &repo, &["--base", "main"]);
    assert_eq!(
        from_base["parts"][0]["names"],
        value!(["formatId", "formatUser"])
    );
    assert_eq!(reached(&from_base), EVERYONE);
}

#[scenario(
    "cli",
    "--changed follows a changed method to its callers and not to the callers of another"
)]
#[test]
fn changed_follows_a_changed_method_to_its_callers_and_not_to_the_callers_of_another() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    let service = "export class Service {\n  static make(): Service {\n    return new Service();\n  }\n\n  static place(order: string): string {\n    return order;\n  }\n}\n";
    write(&repo, "src/service.ts", service);
    write(
        &repo,
        "src/places.ts",
        "import { Service } from './service';\n\nexport function placeOrder(order: string) {\n  return Service.place(order);\n}\n",
    );
    write(
        &repo,
        "src/makes.ts",
        "import { Service } from './service';\n\nexport function makeService() {\n  return Service.make();\n}\n",
    );
    commit_all(&repo);
    write(
        &repo,
        "src/service.ts",
        &service.replace("return order;", "return order.trim();"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), ["src/places.ts"]);
    assert_eq!(answer["parts"][0]["names"], value!(["Service.place"]));
}

#[scenario(
    "cli",
    "--changed walks a file whole when its new version does not parse"
)]
#[test]
fn changed_walks_a_file_whole_when_its_new_version_does_not_parse() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    write(
        &repo,
        "src/format.ts",
        &FORMAT.replace(
            "formatUser(name: string): string {",
            "formatUser(name: string {{",
        ),
    );

    let run = sandbox.run_in(&repo, &["--json", "impact", "--changed"]);

    assert_eq!(run.code, Some(0), "{run:?}");
    let answer = json(&run);
    assert_eq!(reached(&answer), EVERYONE, "{answer}");
    assert!(answer.get("parts").is_none(), "{answer}");
}

#[scenario("cli", "--changed walks a file whole when a declaration moves")]
#[test]
fn changed_walks_a_file_whole_when_a_declaration_moves() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    let (user, id) = FORMAT.split_once("\nexport function formatId").unwrap();
    write(
        &repo,
        "src/format.ts",
        &format!("export function formatId{id}\n{user}"),
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert_eq!(reached(&answer), EVERYONE, "{answer}");
    assert!(answer.get("parts").is_none(), "{answer}");
}

#[scenario("cli", "--changed reads the whitespace inside a template as a change")]
#[test]
fn changed_reads_the_whitespace_inside_a_template_as_a_change() {
    let sandbox = Sandbox::new();
    let repo = formatting(&sandbox);
    let query = "export function render(id: number): string {\n  return `\n    SELECT *\n      FROM users\n     WHERE id = ${id}\n  `;\n}\n\nexport function quiet(): number {\n  return 1;\n}\n";
    write(&repo, "src/sql.ts", query);
    write(
        &repo,
        "src/sql-user.ts",
        "import { render } from './sql';\n\nexport const text = render(1);\n",
    );
    commit_all(&repo);

    write(
        &repo,
        "src/sql.ts",
        &query.replace("      FROM", "         FROM"),
    );
    let inside = changed(&sandbox, &repo, &[]);
    assert_eq!(reached(&inside), ["src/sql-user.ts"], "{inside}");
    assert_eq!(inside["parts"][0]["names"], value!(["render"]));

    write(
        &repo,
        "src/sql.ts",
        &query.replace("  return 1;", "      return 1;"),
    );
    let outside = changed(&sandbox, &repo, &[]);
    assert_eq!(reached(&outside), Vec::<&str>::new(), "{outside}");
    assert_eq!(
        outside["parts"],
        value!([{ "file": "src/sql.ts", "names": [] }])
    );
}

/// The real connector reading classes that take the members of another: a subclass in the file of its base, a generated-style subclass whose instance a constructor receives, and a constant that holds an instance.
#[test]
fn a_member_passes_on_to_what_takes_it_in_typescript() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/filemap.ts",
        "export class FileMap<V> {\n  set(key: string, value?: V) {}\n  get(key: string) {}\n}\n\nexport class ModuleMap extends FileMap<string> {}\n",
    );
    write(
        &repo,
        "src/loader.ts",
        "import { ModuleMap } from './filemap';\n\nexport function load(file: string) {\n  new ModuleMap().set(file);\n}\n",
    );
    write(
        &repo,
        "src/version-base.ts",
        "export class VersionServiceBase {\n  count() {}\n  reset() {}\n}\n",
    );
    write(
        &repo,
        "src/version.ts",
        "import { VersionServiceBase } from './version-base';\n\nexport class VersionService extends VersionServiceBase {}\n",
    );
    write(
        &repo,
        "src/totals.ts",
        "import { VersionService } from './version';\n\nexport class Totals {\n  constructor(private readonly service: VersionService) {}\n\n  sum() {\n    return this.service.count();\n  }\n}\n",
    );
    write(
        &repo,
        "src/logger.ts",
        "export class Logger {\n  error() {}\n  info() {}\n}\n",
    );
    write(
        &repo,
        "src/log.ts",
        "import { Logger } from './logger';\n\nexport const logger = new Logger();\n",
    );
    write(
        &repo,
        "src/failure.ts",
        "import { logger } from './log';\n\nexport function fail() {\n  logger.error();\n}\n",
    );
    write(
        &repo,
        "src/notice.ts",
        "import { logger } from './log';\n\nexport function notice() {\n  logger.info();\n}\n",
    );

    let owned = |names: &[&str]| {
        names
            .iter()
            .map(|&name| name.to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        callers_in(&sandbox, &repo, "src/filemap.ts", "FileMap.set"),
        [("src/loader.ts".to_owned(), owned(&["load"]))],
        "`ModuleMap` extends the class, a use of the class alone in a file that never names `set` on a value"
    );
    assert_eq!(
        callers_in(
            &sandbox,
            &repo,
            "src/version-base.ts",
            "VersionServiceBase.count"
        ),
        [("src/totals.ts".to_owned(), owned(&["Totals.sum"]))],
        "`VersionService` extends the class and never names `count`"
    );
    assert_eq!(
        callers_in(&sandbox, &repo, "src/logger.ts", "Logger.error"),
        [("src/failure.ts".to_owned(), owned(&["fail"]))],
        "`notice.ts` calls `info`, and the constant that holds the instance never names `error`"
    );
}

/// A Go module whose package `invoice` has `Header` in `header.go` and `Footer` in `footer.go`, and a `Service` with two methods.
fn go_module(dir: &Path) {
    write(dir, "go.mod", "module example.com/shop\n\ngo 1.22\n");
    write(
        dir,
        "invoice/header.go",
        "package invoice\n\nfunc Header() string { return \"h\" }\n",
    );
    write(
        dir,
        "invoice/footer.go",
        "package invoice\n\nfunc Footer() string { return \"f\" }\n",
    );
    write(
        dir,
        "invoice/service.go",
        "package invoice\n\ntype Service struct{}\n\nfunc (s *Service) Place() {}\n\nfunc (s *Service) Cancel() {}\n",
    );
    write(
        dir,
        "report.go",
        "package main\n\nimport \"example.com/shop/invoice\"\n\nfunc Print() string {\n\treturn invoice.Header()\n}\n",
    );
    write(
        dir,
        "ship.go",
        "package main\n\nimport \"example.com/shop/invoice\"\n\nfunc Ship() string {\n\treturn invoice.Footer()\n}\n",
    );
    write(
        dir,
        "order.go",
        "package main\n\nimport \"example.com/shop/invoice\"\n\nfunc Order(s *invoice.Service) {\n\ts.Place()\n}\n",
    );
    write(
        dir,
        "refund.go",
        "package main\n\nimport \"example.com/shop/invoice\"\n\nfunc Refund(s *invoice.Service) {\n\ts.Cancel()\n}\n",
    );
}

#[scenario(
    "queries",
    "A Go caller of a function is listed and a caller of another function is not"
)]
#[test]
fn a_go_caller_of_a_function_is_listed_and_a_caller_of_another_function_is_not() {
    let sandbox = Sandbox::new();
    go_module(sandbox.work.path());

    let answer = json(&sandbox.run(&["--json", "impact", "invoice/header.go", "--name", "Header"]));
    assert_eq!(paths(&answer["affected"]), ["report.go"]);
    assert_eq!(
        answer["affected"][0]["callers"],
        value!([{ "name": "Print", "line": 5, "distance": 1 }])
    );
}

#[scenario(
    "queries",
    "A Go caller of a method is told apart from a caller of another method"
)]
#[test]
fn a_go_caller_of_a_method_is_told_apart_from_a_caller_of_another_method() {
    let sandbox = Sandbox::new();
    go_module(sandbox.work.path());

    let place = json(&sandbox.run(&[
        "--json",
        "impact",
        "invoice/service.go",
        "--name",
        "Service.Place",
    ]));
    let cancel = json(&sandbox.run(&[
        "--json",
        "impact",
        "invoice/service.go",
        "--name",
        "Service.Cancel",
    ]));
    let callers = |answer: &Value| -> Vec<String> {
        answer["affected"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|entry| {
                entry["callers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|caller| caller["name"].as_str().unwrap().to_owned())
            })
            .collect()
    };
    assert_eq!(callers(&place), ["Order"], "{place}");
    assert_eq!(callers(&cancel), ["Refund"], "{cancel}");
}

#[scenario(
    "queries",
    "A Go method declared in a file of its own is found through its type"
)]
#[test]
fn a_go_method_declared_in_a_file_of_its_own_is_found_through_its_type() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    write(dir, "go.mod", "module example.com/shop\n\ngo 1.22\n");
    write(
        dir,
        "invoice/service.go",
        "package invoice\n\ntype Service struct{}\n",
    );
    write(
        dir,
        "invoice/charge.go",
        "package invoice\n\nfunc (s *Service) Charge() {}\n",
    );
    write(
        dir,
        "order.go",
        "package main\n\nimport \"example.com/shop/invoice\"\n\nfunc Order(s *invoice.Service) {\n\ts.Charge()\n}\n",
    );

    let answer = json(&sandbox.run(&[
        "--json",
        "impact",
        "invoice/charge.go",
        "--name",
        "Service.Charge",
    ]));
    assert_eq!(paths(&answer["affected"]), ["order.go"], "{answer}");
}

#[scenario("go", "A file with methods of a type does not pass the type on")]
#[test]
fn a_file_with_methods_of_a_type_does_not_pass_the_type_on() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    write(dir, "go.mod", "module example.com/shop\n\ngo 1.22\n");
    write(
        dir,
        "command.go",
        "package shop\n\ntype Command struct{}\n\nfunc (c *Command) Run() { c.Init() }\n",
    );
    write(
        dir,
        "init.go",
        "package shop\n\nfunc (c *Command) Init() {}\n",
    );
    write(
        dir,
        "other.go",
        "package shop\n\nfunc (c *Command) Other() {}\n",
    );

    let answer = json(&sandbox.run(&["--json", "impact", "init.go", "--name", "Command.Init"]));
    assert_eq!(paths(&answer["affected"]), ["command.go"], "{answer}");
}

#[scenario("go", "An edge of a package carries only the names its target exports")]
#[test]
fn an_edge_of_a_package_carries_only_the_names_its_target_exports() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    write(dir, "go.mod", "module example.com/shop\n\ngo 1.22\n");
    write(
        dir,
        "a.go",
        "package shop\n\nfunc A(xs []int) int { return len(xs) + Helper() }\n",
    );
    write(
        dir,
        "b.go",
        "package shop\n\nfunc Helper() int { return 1 }\n",
    );
    write(dir, "c.go", "package shop\n\nfunc Other() {}\n");

    let answer = json(&sandbox.run(&["--json", "dependencies", "a.go"]));
    let text = answer.to_string();
    assert!(text.contains("Helper"), "{text}");
    assert!(!text.contains("len"), "{text}");
}

#[scenario("queries", "A Go client meets a route served in another language")]
#[test]
fn a_go_client_meets_a_route_served_in_another_language() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    write(
        dir,
        "api/package.json",
        r#"{ "name": "api", "dependencies": { "express": "^4.0.0" } }"#,
    );
    write(
        dir,
        "api/src/users.ts",
        "import express from 'express';\nconst app = express();\napp.get('/api/users', (req, res) => res.json([]));\n",
    );
    write(dir, "billing/go.mod", "module example.com/billing\n");
    write(
        dir,
        "billing/main.go",
        "package main\n\nimport \"net/http\"\n\nfunc main() {\n\tbase := \"http://api\"\n\thttp.Get(base + \"/api/users\")\n}\n",
    );

    let found = json(&sandbox.run(&["--json", "contract", "http GET /api/users"]));
    let site = |role: &str| -> Vec<String> {
        found["found"][role]
            .as_array()
            .map(|sites| {
                sites
                    .iter()
                    .map(|site| site["path"].as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(site("serves"), ["api/src/users.ts"], "{found}");
    assert_eq!(site("calls"), ["billing/main.go"], "{found}");

    let impact = json(&sandbox.run(&["--json", "impact", "api/src/users.ts"]));
    assert_eq!(paths(&impact["affected"]), ["billing/main.go"], "{impact}");
}

/// The `impact --name` answer for `name` of `file` on a Go module of `files`.
fn go_answer(files: &[(&str, &str)], file: &str, name: &str) -> Value {
    let sandbox = Sandbox::new();
    write(
        sandbox.work.path(),
        "go.mod",
        "module example.com/m\n\ngo 1.22\n",
    );
    for (path, text) in files {
        write(sandbox.work.path(), path, text);
    }
    json(&sandbox.run(&["--json", "impact", file, "--name", name]))
}

/// A provider interface with two methods, two classes that implement it, and the files that hold one of them.
fn write_providers(repo: &Path) {
    write(
        repo,
        "src/provider.ts",
        "export interface GitProvider {\n  getGitGroups(): string[];\n  other(): void;\n}\n",
    );
    write(
        repo,
        "src/aws.ts",
        "import type { GitProvider } from './provider';\n\nexport class Aws implements GitProvider {\n  getGitGroups() {\n    return [];\n  }\n  other() {}\n  extra() {}\n}\n",
    );
    write(
        repo,
        "src/github.ts",
        "import type { GitProvider } from './provider';\n\nexport class Github implements GitProvider {\n  getGitGroups() {\n    return [];\n  }\n  other() {}\n}\n",
    );
    write(
        repo,
        "src/app.ts",
        "import type { GitProvider } from './provider';\n\nexport function groups(provider: GitProvider) {\n  return provider.getGitGroups();\n}\n",
    );
    write(
        repo,
        "src/rest.ts",
        "import type { GitProvider } from './provider';\n\nexport function rest(provider: GitProvider) {\n  return provider.other();\n}\n",
    );
    write(
        repo,
        "src/only-github.ts",
        "import { Github } from './github';\n\nexport function only(github: Github) {\n  return github.getGitGroups();\n}\n",
    );
}

fn ask(sandbox: &Sandbox, repo: &Path, path: &str, name: &str) -> Value {
    json(&sandbox.run_in(repo, &["--json", "impact", path, "--name", name]))
}

#[scenario(
    "queries",
    "A method called on what a function returns reaches the caller"
)]
#[test]
fn a_method_called_on_what_a_function_returns_reaches_the_caller() {
    let answer = go_answer(
        &[
            (
                "lib/mux.go",
                "package lib\n\ntype Mux struct{}\n\nfunc (m *Mux) Get() {}\n\nfunc (m *Mux) Put() {}\n",
            ),
            (
                "lib/new.go",
                "package lib\n\nfunc NewRouter() *Mux { return &Mux{} }\n",
            ),
            (
                "app/uses.go",
                "package app\n\nimport \"example.com/m/lib\"\n\nfunc Run() {\n\tr := lib.NewRouter()\n\tr.Get()\n}\n",
            ),
            (
                "app/other.go",
                "package app\n\nimport \"example.com/m/lib\"\n\nfunc Other() {\n\tr := lib.NewRouter()\n\tr.Put()\n}\n",
            ),
        ],
        "lib/mux.go",
        "Mux.Get",
    );
    assert_eq!(paths(&answer["affected"]), ["app/uses.go"], "{answer}");
    assert_eq!(
        answer["notes"],
        value!(["2 files hold Mux and read no Get through it; ask about Mux to list them"]),
        "`app/other.go` reads `Put` and `lib/new.go` reads nothing"
    );
}

#[scenario("queries", "A method promoted through embedding reaches the caller")]
#[test]
fn a_method_promoted_through_embedding_reaches_the_caller() {
    let answer = go_answer(
        &[
            (
                "lib/group.go",
                "package lib\n\ntype RouterGroup struct{}\n\nfunc (g *RouterGroup) PATCH() {}\n\nfunc (g *RouterGroup) GET() {}\n",
            ),
            (
                "lib/engine.go",
                "package lib\n\ntype Engine struct {\n\tRouterGroup\n}\n\nfunc New() *Engine { return &Engine{} }\n",
            ),
            (
                "app/uses.go",
                "package app\n\nimport \"example.com/m/lib\"\n\nfunc Run() {\n\te := lib.New()\n\te.PATCH()\n}\n",
            ),
            (
                "app/other.go",
                "package app\n\nimport \"example.com/m/lib\"\n\nfunc Other() {\n\te := lib.New()\n\te.GET()\n}\n",
            ),
        ],
        "lib/group.go",
        "RouterGroup.PATCH",
    );
    assert_eq!(paths(&answer["affected"]), ["app/uses.go"], "{answer}");
}

#[scenario("queries", "A method through a package variable reaches the caller")]
#[test]
fn a_method_through_a_package_variable_reaches_the_caller() {
    let answer = go_answer(
        &[
            (
                "lib/v2.go",
                "package lib\n\ntype v2 struct{}\n\nfunc (v v2) Make() {}\n\nfunc (v v2) Drop() {}\n\nvar V2 = v2{}\n",
            ),
            (
                "app/uses.go",
                "package app\n\nimport \"example.com/m/lib\"\n\nfunc Run() {\n\tlib.V2.Make()\n}\n",
            ),
            (
                "app/other.go",
                "package app\n\nimport \"example.com/m/lib\"\n\nfunc Other() {\n\tlib.V2.Drop()\n}\n",
            ),
        ],
        "lib/v2.go",
        "v2.Make",
    );
    assert_eq!(paths(&answer["affected"]), ["app/uses.go"], "{answer}");
}

#[scenario("queries", "A router required in place reaches a fetch of its route")]
#[test]
fn a_router_required_in_place_reaches_a_fetch_of_its_route() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    write(
        dir,
        "package.json",
        r#"{ "name": "api", "dependencies": { "express": "^4.0.0" } }"#,
    );
    write(
        dir,
        "src/a.js",
        "const router = require('express').Router();\nrouter.get('/a', (req, res) => res.end());\nmodule.exports = router;\n",
    );
    write(
        dir,
        "src/b.js",
        "const app = require('express')();\napp.use('/api', require('./a'));\n",
    );
    write(
        dir,
        "web/page.ts",
        "export function load() {\n  return fetch('/api/a');\n}\n",
    );

    let impact = json(&sandbox.run(&["--json", "impact", "src/a.js"]));
    assert!(
        paths(&impact["affected"]).contains(&"web/page.ts"),
        "{impact}"
    );
}

#[scenario(
    "queries",
    "A caller through an interface is a caller of the method that implements it"
)]
#[test]
fn a_caller_through_an_interface_is_a_caller_of_the_method_that_implements_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);

    let answer = ask(&sandbox, &repo, "src/aws.ts", "Aws.getGitGroups");
    assert_eq!(
        paths(&answer["affected"]),
        ["src/app.ts", "src/provider.ts"],
        "{answer}"
    );
    assert_eq!(
        answer["affected"][0]["callers"],
        value!([{ "name": "groups", "line": 3, "distance": 1 }])
    );
    assert!(
        answer["notes"]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|note| note
                == "Aws.getGitGroups may run through GitProvider.getGitGroups in src/provider.ts, so its callers are listed")),
        "{answer}"
    );
}

#[scenario("queries", "The member an override overrides is listed")]
#[test]
fn the_member_an_override_overrides_is_listed() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);

    let answer = ask(&sandbox, &repo, "src/aws.ts", "Aws.getGitGroups");
    let row = answer["affected"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["path"] == "src/provider.ts"));
    assert_eq!(
        row.map(|row| &row["reason"]),
        Some(&value!({ "via": "declares", "names": ["GitProvider.getGitGroups"] })),
        "{answer}"
    );
    assert!(
        answer["notes"]
            .as_array()
            .is_some_and(|notes| !notes.is_empty()),
        "{answer}"
    );
}

#[scenario(
    "queries",
    "A caller through a base class is a caller of the method that overrides it"
)]
#[test]
fn a_caller_through_a_base_class_is_a_caller_of_the_method_that_overrides_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(&repo, "src/base.ts", "export class Base {\n  run() {}\n}\n");
    write(
        &repo,
        "src/sub.ts",
        "import { Base } from './base';\n\nexport class Sub extends Base {\n  run() {}\n}\n",
    );
    write(
        &repo,
        "src/app.ts",
        "import { Base } from './base';\n\nexport function go(base: Base) {\n  base.run();\n}\n",
    );

    let answer = ask(&sandbox, &repo, "src/sub.ts", "Sub.run");
    assert!(
        paths(&answer["affected"]).contains(&"src/app.ts"),
        "{answer}"
    );
}

#[scenario("queries", "A sibling implementation is no caller")]
#[test]
fn a_sibling_implementation_is_no_caller() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);

    let answer = ask(&sandbox, &repo, "src/aws.ts", "Aws.getGitGroups");
    let listed = paths(&answer["affected"]);
    assert!(!listed.contains(&"src/github.ts"), "{answer}");
    assert!(!listed.contains(&"src/only-github.ts"), "{answer}");
    assert!(!listed.contains(&"src/rest.ts"), "{answer}");
}

#[scenario("queries", "A sibling implementation is not listed")]
#[test]
fn a_sibling_implementation_is_not_listed() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);

    let answer = ask(&sandbox, &repo, "src/aws.ts", "Aws.getGitGroups");
    assert!(
        !paths(&answer["affected"]).contains(&"src/github.ts"),
        "{answer}"
    );
}

#[scenario(
    "queries",
    "A method the interface does not declare gets nothing from it"
)]
#[test]
fn a_method_the_interface_does_not_declare_gets_nothing_from_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);
    write(
        &repo,
        "src/extra.ts",
        "import type { GitProvider } from './provider';\n\nexport function extra(provider: GitProvider) {\n  return (provider as any).extra();\n}\n",
    );

    let answer = ask(&sandbox, &repo, "src/aws.ts", "Aws.extra");
    assert!(
        !paths(&answer["affected"]).contains(&"src/extra.ts"),
        "{answer}"
    );
    assert!(
        answer.get("notes").is_none_or(Value::is_null),
        "no note about a head: {answer}"
    );
}

#[scenario("queries", "A head is followed through a barrel")]
#[test]
fn a_head_is_followed_through_a_barrel() {
    for barrel in [
        "export { GitProvider } from './provider';\n",
        "export * from './provider';\n",
    ] {
        let sandbox = Sandbox::new();
        let repo = sandbox.repository();
        write_providers(&repo);
        write(&repo, "src/index.ts", barrel);
        write(
            &repo,
            "src/aws.ts",
            "import type { GitProvider } from './index';\n\nexport class Aws implements GitProvider {\n  getGitGroups() {\n    return [];\n  }\n}\n",
        );

        let answer = ask(&sandbox, &repo, "src/aws.ts", "Aws.getGitGroups");
        assert!(
            paths(&answer["affected"]).contains(&"src/app.ts"),
            "{barrel}: {answer}"
        );
        assert!(
            answer["notes"]
                .as_array()
                .is_some_and(|notes| notes.iter().any(|note| note
                    == "Aws.getGitGroups may run through GitProvider.getGitGroups in src/provider.ts, so its callers are listed")),
            "{barrel}: {answer}"
        );
    }
}

#[scenario("queries", "A changed method is followed through what it implements")]
#[test]
fn a_changed_method_is_followed_through_what_it_implements() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);
    commit_all(&repo);
    write(
        &repo,
        "src/aws.ts",
        "import type { GitProvider } from './provider';\n\nexport class Aws implements GitProvider {\n  getGitGroups() {\n    return ['changed'];\n  }\n  other() {}\n  extra() {}\n}\n",
    );

    let answer = json(&sandbox.run_in(&repo, &["--json", "impact", "--changed"]));
    let listed = paths(&answer["affected"]);
    assert!(listed.contains(&"src/app.ts"), "{answer}");
    assert!(!listed.contains(&"src/rest.ts"), "{answer}");
    assert!(!listed.contains(&"src/only-github.ts"), "{answer}");
}

#[scenario(
    "queries",
    "Callers of a member of an interface are the holders that name it"
)]
#[test]
fn callers_of_a_member_of_an_interface_are_the_holders_that_name_it() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write_providers(&repo);

    let answer = ask(
        &sandbox,
        &repo,
        "src/provider.ts",
        "GitProvider.getGitGroups",
    );
    let listed = paths(&answer["affected"]);
    assert!(listed.contains(&"src/app.ts"), "{answer}");
    assert!(!listed.contains(&"src/rest.ts"), "{answer}");
    assert!(
        answer["notes"]
            .as_array()
            .is_none_or(|notes| !notes.iter().any(|note| note
                .as_str()
                .is_some_and(|note| note.contains("declares no")))),
        "{answer}"
    );
}

/// A router of one package that an app of another package mounts is served under the mount, so a change to the router reaches the client of the mounted path.
#[test]
#[scenario(
    "queries",
    "A router of another package reaches the client of the mounted path"
)]
fn a_router_of_another_package_reaches_the_client_of_the_mounted_path() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "package.json",
        "{ \"private\": true, \"workspaces\": [\"packages/*\"] }\n",
    );
    write(
        &repo,
        "packages/routes/package.json",
        "{ \"name\": \"@acme/routes\", \"main\": \"src/index.ts\", \"dependencies\": { \"express\": \"4\" } }\n",
    );
    write(
        &repo,
        "packages/routes/src/index.ts",
        "import { Router } from 'express';\n\nexport const shared = Router();\nshared.get('/x', (req, res) => res.end());\n",
    );
    write(
        &repo,
        "packages/api/package.json",
        "{ \"name\": \"@acme/api\", \"dependencies\": { \"@acme/routes\": \"1.0.0\", \"express\": \"4\" } }\n",
    );
    write(
        &repo,
        "packages/api/src/app.ts",
        "import express from 'express';\nimport { shared } from '@acme/routes';\n\nconst app = express();\napp.use('/ext', shared);\n",
    );
    write(
        &repo,
        "packages/web/package.json",
        "{ \"name\": \"@acme/web\" }\n",
    );
    write(
        &repo,
        "packages/web/src/client.ts",
        "export const load = () => fetch('/ext/x');\n",
    );

    let answer =
        json(&sandbox.run_in(&repo, &["--json", "impact", "packages/routes/src/index.ts"]));
    assert_eq!(
        paths(&answer["affected"]),
        ["packages/api/src/app.ts", "packages/web/src/client.ts"],
        "{answer}"
    );
    assert_eq!(
        answer["affected"][1]["reason"],
        value!({ "via": "contract", "role": "calls", "key": "http GET /ext/x" }),
        "{answer}"
    );
}

/// Python records no declarations, so a function removed from a module walks the module whole and lists the importer of the function that stays too.
#[test]
fn a_function_removed_from_a_python_module_walks_the_module_whole() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "tools/job.py",
        "def run():\n    return 1\n\n\ndef stop():\n    return 2\n",
    );
    write(&repo, "tools/__init__.py", "");
    write(&repo, "tools/main.py", "from .job import run\n\nrun()\n");
    write(&repo, "tools/halt.py", "from .job import stop\n\nstop()\n");
    commit_all(&repo);
    write(&repo, "tools/job.py", "def stop():\n    return 2\n");

    let answer = changed(&sandbox, &repo, &[]);

    assert!(answer.get("parts").is_none(), "{answer}");
    assert_eq!(
        reached(&answer),
        ["tools/halt.py", "tools/main.py"],
        "{answer}"
    );
}

/// Rust records no declarations either, so a function removed from a module walks the module whole.
#[test]
fn a_function_removed_from_a_rust_module_walks_the_module_whole() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "Cargo.toml",
        "[package]\nname = \"shop\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &repo,
        "src/lib.rs",
        "pub mod job;\npub mod main_user;\npub mod halt;\n",
    );
    write(
        &repo,
        "src/job.rs",
        "pub fn run() -> u8 {\n    1\n}\n\npub fn stop() -> u8 {\n    2\n}\n",
    );
    write(
        &repo,
        "src/main_user.rs",
        "use crate::job::run;\n\npub fn go() -> u8 {\n    run()\n}\n",
    );
    write(
        &repo,
        "src/halt.rs",
        "use crate::job::stop;\n\npub fn go() -> u8 {\n    stop()\n}\n",
    );
    commit_all(&repo);
    write(&repo, "src/job.rs", "pub fn stop() -> u8 {\n    2\n}\n");

    let answer = changed(&sandbox, &repo, &[]);

    assert!(answer.get("parts").is_none(), "{answer}");
    assert_eq!(
        reached(&answer),
        ["src/halt.rs", "src/main_user.rs"],
        "{answer}"
    );
}

/// The Go connector gives no blanked code to compare two versions by declaration, so a function removed from one file of a package walks the file whole and the caller of a function that stays in it is listed too.
#[test]
fn a_function_removed_from_a_go_package_walks_the_file_whole() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    go_module(&repo);
    write(
        &repo,
        "invoice/header.go",
        "package invoice\n\nfunc Header() string { return \"h\" }\n\nfunc Title() string { return \"t\" }\n",
    );
    commit_all(&repo);
    write(
        &repo,
        "invoice/header.go",
        "package invoice\n\nfunc Title() string { return \"t\" }\n",
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert!(answer.get("parts").is_none(), "{answer}");
    assert_eq!(reached(&answer), ["report.go"], "{answer}");
}

/// A rewritten `export { b as c }` is a changed statement, which the comparison cannot tell by declaration, so the file is walked whole.
#[test]
fn a_rewritten_export_alias_walks_the_file_whole() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    write(
        &repo,
        "src/alias.ts",
        "function inner(): number {\n  return 1;\n}\n\nexport { inner as first };\n",
    );
    write(
        &repo,
        "src/use.ts",
        "import { first } from './alias';\n\nexport const x = first();\n",
    );
    write(&repo, "src/other.ts", "import './alias';\n");
    commit_all(&repo);
    write(
        &repo,
        "src/alias.ts",
        "function inner(): number {\n  return 1;\n}\n\nexport { inner as second };\n",
    );

    let answer = changed(&sandbox, &repo, &[]);

    assert!(answer.get("parts").is_none(), "{answer}");
    assert_eq!(reached(&answer), ["src/other.ts", "src/use.ts"], "{answer}");
}

/// A class with `place` and `other`, a static factory and two function factories, one typed and one not, and a caller of each.
fn factory_project(repo: &Path) {
    write(
        repo,
        "src/service.ts",
        "export class Service {\n  static make(): Service { return new Service(); }\n  place() {}\n  other() {}\n}\n\nexport function makeService(): Service { return new Service(); }\n\nexport const made = makeService();\n",
    );
    write(
        repo,
        "src/by-static.ts",
        "import { Service } from './service';\n\nexport function byStatic() {\n  const s = Service.make();\n  s.place();\n}\n",
    );
    write(
        repo,
        "src/by-function.ts",
        "import { makeService } from './service';\n\nexport function byFunction() {\n  const s = makeService();\n  s.place();\n}\n",
    );
    write(
        repo,
        "src/other-member.ts",
        "import { Service } from './service';\nimport { makeService } from './service';\n\nexport function otherMember() {\n  Service.make().other();\n  makeService().other();\n}\n",
    );
    write(
        repo,
        "src/unrelated.ts",
        "import { makeService } from './service';\n\nexport function unrelated() {\n  makeService();\n}\n",
    );
    write(repo, "src/third.ts", "export { made } from './service';\n");
    write(
        repo,
        "src/shared.ts",
        "import { made } from './third';\n\nexport function shared() {\n  made.place();\n}\n",
    );
}

/// The paths `impact` lists for the member `Service.place`.
fn factory_callers(sandbox: &Sandbox, repo: &Path) -> Vec<String> {
    callers_in(sandbox, repo, "src/service.ts", "Service.place")
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

#[scenario(
    "queries",
    "A caller through a static factory and through a function factory is listed"
)]
#[test]
fn a_caller_through_a_static_factory_and_through_a_function_factory_is_listed() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    factory_project(&repo);
    let listed = factory_callers(&sandbox, &repo);
    for path in ["src/by-static.ts", "src/by-function.ts"] {
        assert!(
            listed.iter().any(|listed| listed == path),
            "{path}: {listed:?}"
        );
    }
}

#[scenario(
    "queries",
    "A file that calls a factory and reads no such member is not listed"
)]
#[test]
fn a_file_that_calls_a_factory_and_reads_no_such_member_is_not_listed() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    factory_project(&repo);
    let listed = factory_callers(&sandbox, &repo);
    for path in ["src/other-member.ts", "src/unrelated.ts"] {
        assert!(
            !listed.iter().any(|listed| listed == path),
            "{path}: {listed:?}"
        );
    }
}

#[scenario(
    "queries",
    "A value made by a factory and passed across a third file is a caller"
)]
#[test]
fn a_value_made_by_a_factory_and_passed_across_a_third_file_is_a_caller() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository();
    factory_project(&repo);
    let listed = factory_callers(&sandbox, &repo);
    assert!(
        listed.iter().any(|listed| listed == "src/shared.ts"),
        "{listed:?}"
    );
}
