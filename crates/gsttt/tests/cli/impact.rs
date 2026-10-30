//! `impact` takes paths or `--changed`, passes what is gone as deleted, follows dependents as far as `--depth` lets it, and with `--name` lists the callers of a name.

use std::fs;
use std::path::PathBuf;

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
        text.stdout.starts_with("impact of app/new.ts\n"),
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

    let everything = json(&sandbox.run_in(&repo, &["--json", "impact", "lib/util.ts"]));
    assert_eq!(paths(&everything["affected"]), ["app/a.ts", "app/c.ts"]);
    assert_eq!(everything["depth"], Value::Null);

    let one_hop =
        json(&sandbox.run_in(&repo, &["--json", "impact", "--depth", "1", "lib/util.ts"]));
    assert_eq!(paths(&one_hop["affected"]), ["app/a.ts"]);
    assert_eq!(one_hop["depth"], 1);
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
    assert_eq!(whole["depth"], Value::Null);
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
    assert_eq!(
        answer["notes"],
        value!([
            "callers of Runner.start through a value are found through the declarations that refer to Runner"
        ])
    );

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
    assert!(
        text.stdout.contains(
            "\nnote: callers of Runner.start through a value are found through the declarations that refer to Runner\n"
        ),
        "{text:?}"
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
    assert!(run.stdout.starts_with("impact of no files\n"), "{run:?}");
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
