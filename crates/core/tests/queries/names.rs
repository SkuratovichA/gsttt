//! `impact` asked about names: the direct callers of a declaration, and the callers of callers when a depth is asked.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use gsttt_core::VERSION;
use gsttt_core::contract::Role;
use gsttt_core::engine::snapshot::{Header, Snapshot, Stored, now, write};
use gsttt_core::fact::{FileTags, Import, ImportFlags, ImportKind};
use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{Affected, Answer, Caller, Entries, Impact, ImpactRequest, Load, impact};
use gsttt_core::testing::{Code, Tree, file};
use gsttt_scenario::scenario;
use proptest::prelude::*;
use serde_json::{json, to_value};

use super::impact::{Case, Take, case, exported, file_path};

/// What an empty list of paths is compared with, because clippy asks `assert_eq!` over `is_empty` in a test.
const NONE: [&str; 0] = [];

/// What a note says when the depth ended the walk with callers left to follow.
const STOPPED: &str = "the walk stopped with callers left to follow; ask about a caller by name, or with a larger depth, to go on";

/// Asks about `names` declared in `files`, with the depth of the request left as given.
fn ask_in(index: &Index, files: &[&str], names: &[&str], depth: Option<u32>) -> Impact {
    let request = ImpactRequest {
        files: files.iter().map(|&path| path.to_owned()).collect(),
        names: names.iter().map(|&name| name.to_owned()).collect(),
        depth,
        ..ImpactRequest::default()
    };
    impact(index, Freshness::Fresh, &request)
}

fn ask(index: &Index, path: &str, names: &[&str], depth: Option<u32>) -> Impact {
    ask_in(index, &[path], names, depth)
}

/// The notes of an answer, each whole, as the JSON of the answer holds it.
fn notes(answer: &Impact) -> Vec<String> {
    answer.notes.iter().map(ToString::to_string).collect()
}

/// What a reader of the answer sees of an entry: its path, distance and reason, the names of its callers and the distance of its load when it is reached.
type Row<'a> = (&'a str, u32, String, Vec<&'a str>, Option<u32>);

fn rows(list: &[Affected]) -> Vec<Row<'_>> {
    list.iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.distance,
                entry.reason.to_string(),
                entry
                    .callers
                    .iter()
                    .map(|caller| caller.name.as_str())
                    .collect(),
                entry.load.as_ref().map(|load| load.distance),
            )
        })
        .collect()
}

/// A row of an entry reached through declarations only.
fn row<'a>(path: &'a str, distance: u32, reason: &str, callers: &[&'a str]) -> Row<'a> {
    (path, distance, reason.to_owned(), callers.to_vec(), None)
}

fn paths(list: &[Affected]) -> Vec<&str> {
    list.iter().map(|entry| entry.path.as_str()).collect()
}

/// `a.ts`, which declares `foo` and `bar`, and `b.ts`, which imports both and uses `foo` in `run` and `bar` in `stop`.
fn library() -> Tree {
    Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .declaration("bar", (4, 5), &[]),
        )
        .exports("a.ts", &["foo", "bar"])
        .imports_names("b.ts", "a.ts", &["foo", "bar"])
        .symbols(
            "b.ts",
            Code::new()
                .declaration("run", (3, 5), &["foo"])
                .declaration("stop", (7, 9), &["bar"]),
        )
        .exports("b.ts", &["run", "stop"])
}

/// `a.ts` declaring `foo`, which `b.ts` uses in `run`, and `c.ts` using `run` in `main`.
fn chain() -> Index {
    Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .symbols("b.ts", Code::new().declaration("run", (3, 5), &["foo"]))
        .exports("b.ts", &["run"])
        .imports_names("c.ts", "b.ts", &["run"])
        .symbols("c.ts", Code::new().declaration("main", (1, 3), &["run"]))
        .exports("c.ts", &[])
        .build()
}

#[scenario("queries", "Only the direct callers are listed")]
#[test]
fn only_the_direct_callers_are_listed() {
    let index = library()
        .imports_names("c.ts", "b.ts", &["run"])
        .symbols("c.ts", Code::new().declaration("main", (1, 3), &["run"]))
        .exports("c.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("b.ts", 1, "imports foo from a.ts", &["run"])]
    );
    assert_eq!(paths(&answer.tests), NONE);
}

#[scenario("queries", "Callers in the asked file are listed")]
#[test]
fn callers_in_the_asked_file_are_listed() {
    let index = Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .declaration("helper", (4, 6), &["foo"])
                .declaration("other", (8, 9), &[]),
        )
        .exports("a.ts", &["foo", "helper", "other"])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("a.ts", 1, "declares foo", &["helper"])]
    );
}

#[scenario("queries", "A barrel adds no hop")]
#[test]
fn a_barrel_adds_no_hop() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .re_exports("index.ts", "a.ts", &[("bar", "foo")])
        .symbols("index.ts", Code::new())
        .exports("index.ts", &["bar"])
        .imports_names("page.ts", "index.ts", &["bar"])
        .symbols(
            "page.ts",
            Code::new().declaration("render", (1, 4), &["bar"]),
        )
        .exports("page.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            row("index.ts", 1, "re-exports foo as bar from a.ts", &[]),
            row("page.ts", 1, "imports bar from index.ts", &["render"]),
        ]
    );
}

#[scenario("queries", "A name imported and exported again adds no hop")]
#[test]
fn a_name_imported_and_exported_again_adds_no_hop() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("mid.ts", "a.ts", &["foo"])
        .symbols("mid.ts", Code::new())
        .exports("mid.ts", &["foo"])
        .imports_names("page.ts", "mid.ts", &["foo"])
        .symbols(
            "page.ts",
            Code::new().declaration("render", (1, 4), &["foo"]),
        )
        .exports("page.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            row("mid.ts", 1, "imports foo from a.ts", &[]),
            row("page.ts", 1, "imports foo from mid.ts", &["render"]),
        ]
    );
}

#[scenario("queries", "A name the file does not declare reaches nothing")]
#[test]
fn a_name_the_file_does_not_declare_reaches_nothing() {
    let index = library().build();

    let answer = ask(&index, "a.ts", &["fooo"], None);

    assert!(answer.affected.is_empty() && answer.tests.is_empty());
    assert_eq!(notes(&answer), ["a.ts declares no fooo"]);
    assert!(
        answer
            .text(Entries::Capped)
            .contains("nothing is affected by a change to fooo in a.ts among")
    );
}

#[scenario("queries", "The answer names the callers and their lines")]
#[test]
fn the_answer_names_the_callers_and_their_lines() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .symbols(
            "b.ts",
            Code::new()
                .declaration("run", (3, 5), &["foo"])
                .declaration("stop", (7, 9), &["foo"]),
        )
        .exports("b.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    let text = answer.text(Entries::Capped);
    assert!(
        text.starts_with("impact of foo in a.ts, within 1 hop\n"),
        "{text}"
    );
    assert!(
        text.contains("b.ts  distance 1  imports foo from a.ts; in run (line 3), stop (line 7)\n"),
        "{text}"
    );
    let json = to_value(&answer).unwrap();
    assert_eq!(
        json["affected"][0]["callers"],
        json!([
            { "name": "run", "line": 3, "distance": 1 },
            { "name": "stop", "line": 7, "distance": 1 }
        ])
    );
}

#[scenario("queries", "A name used in a method affects that method")]
#[test]
fn a_name_used_in_a_method_affects_that_method() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("runner.ts", "a.ts", &["foo"])
        .symbols(
            "runner.ts",
            Code::new()
                .declaration("Runner", (1, 12), &[])
                .declaration("Runner.start", (2, 5), &["foo"])
                .declaration("Runner.stop", (7, 10), &[]),
        )
        .exports("runner.ts", &["Runner"])
        .imports_names("app.ts", "runner.ts", &["Runner"])
        .symbols(
            "app.ts",
            Code::new().declaration("main", (1, 3), &["Runner"]),
        )
        .exports("app.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row(
            "runner.ts",
            1,
            "imports foo from a.ts",
            &["Runner.start"]
        )]
    );
}

#[scenario(
    "queries",
    "Callers of a member are found through this and through its class"
)]
#[test]
fn callers_of_a_member_are_found_through_this_and_through_its_class() {
    let index = Tree::new()
        .symbols(
            "runner.ts",
            Code::new()
                .declaration("Runner", (1, 12), &[])
                .declaration("Runner.start", (2, 5), &[])
                .declaration("Runner.stop", (7, 10), &["Runner.start"]),
        )
        .exports("runner.ts", &["Runner"])
        .imports_names("app.ts", "runner.ts", &["Runner"])
        .symbols(
            "app.ts",
            Code::new().declaration("main", (1, 3), &["Runner"]),
        )
        .exports("app.ts", &[])
        .build();

    let answer = ask(&index, "runner.ts", &["Runner.start"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            row("app.ts", 1, "imports Runner from runner.ts", &["main"]),
            row("runner.ts", 1, "declares Runner.start", &["Runner.stop"]),
        ]
    );
    assert_eq!(
        notes(&answer),
        [
            "callers of Runner.start through a value are found through the declarations that refer to Runner"
        ]
    );
    let text = answer.text(Entries::Capped);
    assert!(
        text.contains(
            "\nnote: callers of Runner.start through a value are found through the declarations that refer to Runner\n"
        ),
        "{text}"
    );
}

/// What a note about members reached through their classes says after the members, which stand for many classes at once.
const THROUGH_THEIR_CLASSES: &str =
    "through a value are found through the declarations that refer to their classes";

/// `runner.ts` declares the class `Runner` with the methods `methods`, and `app.ts` has a `main` that refers to the class.
fn runner(methods: &[&str]) -> Index {
    let mut code = Code::new().declaration("Runner", (1, 99), &[]);
    for (number, method) in methods.iter().enumerate() {
        let line = u32::try_from(number).unwrap() * 3 + 2;
        code = code.declaration(&format!("Runner.{method}"), (line, line + 1), &[]);
    }
    Tree::new()
        .symbols("runner.ts", code)
        .exports("runner.ts", &["Runner"])
        .imports_names("app.ts", "runner.ts", &["Runner"])
        .symbols(
            "app.ts",
            Code::new().declaration("main", (1, 3), &["Runner"]),
        )
        .exports("app.ts", &[])
        .build()
}

/// Asks about the methods `methods` of the class `Runner` in `runner.ts`, with the depth of a walk by name left to its default.
fn ask_runner(index: &Index, methods: &[&str]) -> Impact {
    let asked: Vec<String> = methods
        .iter()
        .map(|method| format!("Runner.{method}"))
        .collect();
    let asked: Vec<&str> = asked.iter().map(String::as_str).collect();
    ask(index, "runner.ts", &asked, None)
}

#[scenario("queries", "Members of several classes are one note")]
#[test]
fn members_reached_through_their_classes_are_one_note() {
    let index = Tree::new()
        .symbols(
            "runner.ts",
            Code::new()
                .declaration("ModuleRunner", (1, 4), &[])
                .declaration("ModuleRunner.import", (2, 3), &[])
                .declaration("Runner", (5, 12), &[])
                .declaration("Runner.start", (6, 7), &[])
                .declaration("Runner.stop", (8, 9), &[]),
        )
        .exports("runner.ts", &["ModuleRunner", "Runner"])
        .imports_names("app.ts", "runner.ts", &["Runner"])
        .symbols(
            "app.ts",
            Code::new().declaration("main", (1, 3), &["Runner"]),
        )
        .exports("app.ts", &[])
        .build();

    let answer = ask(
        &index,
        "runner.ts",
        &["Runner.start", "Runner.stop", "ModuleRunner.import"],
        None,
    );

    assert_eq!(paths(&answer.affected), ["app.ts"]);
    let note = format!(
        "callers of ModuleRunner.import, Runner.start and Runner.stop {THROUGH_THEIR_CLASSES}"
    );
    assert_eq!(notes(&answer), [note.as_str()]);
    let text = answer.text(Entries::Capped);
    assert!(text.contains(&format!("\nnote: {note}\n")), "{text}");
    assert_eq!(text.matches("\nnote: ").count(), 1, "{text}");
}

#[test]
fn the_members_of_the_note_are_cut_after_three_unless_all_are_asked_for() {
    let methods = ["start", "stop", "pause", "resume", "reset"];
    let answer = ask_runner(&runner(&methods), &methods);

    assert_eq!(answer.notes.len(), 1, "{:?}", notes(&answer));
    let capped = format!(
        "\nnote: callers of Runner.pause, Runner.reset, Runner.resume and 2 more {THROUGH_THEIR_CLASSES}\n"
    );
    // A text cut to rows cuts the names of a note as a cap does.
    for entries in [Entries::Capped, Entries::First(0), Entries::First(3)] {
        let text = answer.text(entries);
        assert!(text.contains(&capped), "{entries:?}: {text}");
    }
    let whole =
        "callers of Runner.pause, Runner.reset, Runner.resume, Runner.start and Runner.stop";
    let text = answer.text(Entries::All);
    assert!(
        text.contains(&format!("\nnote: {whole} {THROUGH_THEIR_CLASSES}\n")),
        "{text}"
    );
    // The JSON holds the note whole whatever the text cuts, and as a string like every other note.
    assert_eq!(
        to_value(&answer).unwrap()["notes"],
        json!([format!("{whole} {THROUGH_THEIR_CLASSES}")])
    );
}

#[test]
fn members_of_one_class_are_one_note_that_says_their_classes() {
    let answer = ask_runner(&runner(&["start", "stop"]), &["start", "stop"]);

    assert_eq!(
        notes(&answer),
        [format!(
            "callers of Runner.start and Runner.stop {THROUGH_THEIR_CLASSES}"
        )]
    );
}

#[test]
fn a_name_that_is_no_member_adds_no_note_about_classes() {
    let answer = ask(&runner(&["start"]), "runner.ts", &["Runner"], None);

    assert_eq!(paths(&answer.affected), ["app.ts"]);
    assert!(answer.notes.is_empty(), "{:?}", notes(&answer));
}

#[scenario("queries", "A test that calls a name is a direct caller")]
#[test]
fn a_test_that_calls_a_name_is_a_direct_caller() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("a.test.ts", "a.ts", &["foo"])
        .symbols("a.test.ts", Code::new().on_load(&["foo"]))
        .exports("a.test.ts", &[])
        .tag("a.test.ts", FileTags::TEST)
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(paths(&answer.affected), NONE);
    assert_eq!(
        rows(&answer.tests),
        [(
            "a.test.ts",
            1,
            "imports foo from a.ts".to_owned(),
            Vec::new(),
            Some(1)
        )]
    );
    let text = answer.text(Entries::Capped);
    assert!(
        text.contains("a.test.ts  distance 1  imports foo from a.ts; at load\n"),
        "{text}"
    );
}

#[scenario("queries", "A file reached at load reaches the files that load it")]
#[test]
fn a_file_reached_at_load_reaches_the_files_that_load_it() {
    let type_only = Import {
        names: vec!["Config".to_owned()],
        exported: vec!["Config".to_owned()],
        flags: ImportFlags::TYPE_ONLY,
        ..Import::new("config.ts", ImportKind::Static, 1)
    };
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("load", (1, 3), &[]))
        .exports("a.ts", &["load"])
        .imports_names("config.ts", "a.ts", &["load"])
        .symbols(
            "config.ts",
            Code::new()
                .declaration("config", (1, 1), &["load"])
                .declaration("Config", (3, 3), &[])
                .on_load(&["load"]),
        )
        .exports("config.ts", &["config", "Config"])
        .imports_names("app.ts", "config.ts", &["config"])
        .symbols("app.ts", Code::new())
        .exports("app.ts", &[])
        .import("types.ts", type_only, file("config.ts"))
        .symbols(
            "types.ts",
            Code::new().declaration("Alias", (1, 1), &["Config"]),
        )
        .exports("types.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["load"], Some(2));

    assert_eq!(
        rows(&answer.affected),
        [
            (
                "config.ts",
                1,
                "imports load from a.ts".to_owned(),
                vec!["config"],
                Some(1)
            ),
            (
                "app.ts",
                2,
                "imports config.ts".to_owned(),
                Vec::new(),
                Some(2)
            ),
        ]
    );
}

#[scenario("queries", "A file reached at load reaches the files that mock it")]
#[test]
fn a_file_reached_at_load_reaches_the_files_that_mock_it() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("load", (1, 3), &[]))
        .exports("a.ts", &["load"])
        .imports_names("config.ts", "a.ts", &["load"])
        .symbols(
            "config.ts",
            Code::new()
                .declaration("config", (1, 1), &["load"])
                .on_load(&["load"]),
        )
        .exports("config.ts", &["config"])
        .import(
            "config.test.ts",
            Import::new("config.ts", ImportKind::Mock, 1),
            file("config.ts"),
        )
        .symbols("config.test.ts", Code::new())
        .exports("config.test.ts", &[])
        .tag("config.test.ts", FileTags::TEST)
        .build();

    let answer = ask(&index, "a.ts", &["load"], Some(2));

    assert_eq!(paths(&answer.affected), ["config.ts"]);
    assert_eq!(
        rows(&answer.tests),
        [(
            "config.test.ts",
            2,
            "imports config.ts".to_owned(),
            Vec::new(),
            Some(2)
        )]
    );
}

#[scenario("queries", "A larger depth follows the callers of callers")]
#[test]
fn a_larger_depth_follows_the_callers_of_callers() {
    let answer = ask(&chain(), "a.ts", &["foo"], Some(2));

    assert_eq!(
        rows(&answer.affected),
        [
            row("b.ts", 1, "imports foo from a.ts", &["run"]),
            row("c.ts", 2, "imports run from b.ts", &["main"]),
        ]
    );
    assert_eq!(answer.depth, Some(2));
}

#[scenario("queries", "A caller reached farther than its file says its distance")]
#[test]
fn each_caller_and_the_load_carry_the_distance_they_were_reached_at() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .symbols(
            "b.ts",
            Code::new()
                .declaration("inner", (1, 3), &["foo"])
                .declaration("outer", (5, 7), &["inner"])
                .on_load(&["inner"]),
        )
        .exports("b.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], Some(2));

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    let entry = &answer.affected[0];
    assert_eq!(entry.distance, 1);
    assert_eq!(
        entry.callers,
        [
            Caller {
                name: "inner".to_owned(),
                line: 1,
                distance: 1
            },
            Caller {
                name: "outer".to_owned(),
                line: 5,
                distance: 2
            },
        ]
    );
    assert_eq!(entry.load, Some(Load { distance: 2 }));
    let text = answer.text(Entries::Capped);
    assert!(
        text.contains(
            "b.ts  distance 1  imports foo from a.ts; in inner (line 1), outer (line 5, distance 2); at load (distance 2)\n"
        ),
        "{text}"
    );
    let json = to_value(&answer).unwrap();
    assert_eq!(
        json["affected"][0]["callers"],
        json!([
            { "name": "inner", "line": 1, "distance": 1 },
            { "name": "outer", "line": 5, "distance": 2 }
        ])
    );
    assert_eq!(json["affected"][0]["load"], json!({ "distance": 2 }));
    // One hop reaches `inner` and not yet the load, and reads as it did before distances were said.
    let near = ask(&index, "a.ts", &["foo"], Some(1)).text(Entries::Capped);
    assert!(
        near.contains("b.ts  distance 1  imports foo from a.ts; in inner (line 1)\n"),
        "{near}"
    );
}

#[scenario("queries", "The answer says when the walk stopped at the callers")]
#[test]
fn the_answer_says_when_the_walk_stopped_at_the_callers() {
    let answer = ask(&chain(), "a.ts", &["foo"], None);

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert_eq!(notes(&answer), [STOPPED]);
    assert!(
        answer
            .text(Entries::Capped)
            .contains(&format!("note: {STOPPED}\n"))
    );
}

#[scenario("queries", "A walk that runs out says nothing about stopping")]
#[test]
fn a_walk_that_runs_out_says_nothing_about_stopping() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .symbols("b.ts", Code::new().declaration("run", (3, 5), &["foo"]))
        .exports("b.ts", &["run"])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert!(answer.notes.is_empty(), "{:?}", notes(&answer));
}

#[scenario("queries", "A Python file asked about by name is followed whole")]
#[test]
fn a_python_file_asked_about_by_name_is_followed_whole() {
    let index = Tree::new()
        .read_by("python", "lib.py")
        .read_by("python", "user.py")
        .read_by("python", "app.py")
        .imports_names("user.py", "lib.py", &["parse"])
        .imports("app.py", "user.py")
        .build();

    let answer = ask(&index, "lib.py", &["parse"], None);

    assert_eq!(paths(&answer.affected), ["user.py"]);
    assert!(
        notes(&answer).contains(
            &"lib.py is followed whole because its connector records no declarations".to_owned()
        ),
        "{:?}",
        notes(&answer)
    );
}

#[scenario("queries", "An importer without declarations is followed whole")]
#[test]
fn an_importer_without_declarations_is_followed_whole() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .recovered("b.ts", 1, 3)
        .imports_names("c.ts", "b.ts", &["bar"])
        .symbols("c.ts", Code::new().declaration("main", (1, 3), &["bar"]))
        .exports("c.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], Some(2));

    assert_eq!(
        rows(&answer.affected),
        [
            row("b.ts", 1, "imports foo from a.ts", &[]),
            row("c.ts", 2, "imports bar from b.ts", &["main"]),
        ]
    );
}

#[scenario("queries", "A mock takes no name")]
#[test]
fn a_mock_takes_no_name() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .import(
            "a.test.ts",
            Import::new("a.ts", ImportKind::Mock, 1),
            file("a.ts"),
        )
        .symbols("a.test.ts", Code::new())
        .exports("a.test.ts", &[])
        .tag("a.test.ts", FileTags::TEST)
        .build();

    let answer = ask(&index, "a.ts", &["foo"], Some(2));

    assert_eq!(paths(&answer.affected), NONE);
    assert_eq!(paths(&answer.tests), NONE);
}

#[scenario("queries", "A mock of a file followed whole is a caller")]
#[test]
fn a_mock_of_a_file_followed_whole_is_a_caller() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .recovered("b.ts", 1, 3)
        .import(
            "b.test.ts",
            Import::new("b.ts", ImportKind::Mock, 1),
            file("b.ts"),
        )
        .symbols("b.test.ts", Code::new())
        .exports("b.test.ts", &[])
        .tag("b.test.ts", FileTags::TEST)
        .build();

    let answer = ask(&index, "a.ts", &["foo"], Some(2));

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert_eq!(
        rows(&answer.tests),
        [(
            "b.test.ts",
            2,
            "imports b.ts".to_owned(),
            Vec::new(),
            Some(2)
        )]
    );
}

#[scenario("queries", "A declaration crosses only its own contracts")]
#[test]
fn a_declaration_crosses_only_its_own_contracts() {
    let index = Tree::new()
        .symbols(
            "api.ts",
            Code::new()
                .declaration("listUsers", (1, 5), &[])
                .declaration("health", (7, 10), &[]),
        )
        .exports("api.ts", &["listUsers", "health"])
        .site("api.ts", "http GET /users", Role::Serves, 3)
        .site("api.ts", "http GET /health", Role::Serves, 8)
        .symbols(
            "users.ts",
            Code::new().declaration("fetchUsers", (1, 4), &[]),
        )
        .exports("users.ts", &[])
        .site("users.ts", "http GET /users", Role::Calls, 2)
        .symbols("ping.ts", Code::new().declaration("ping", (1, 4), &[]))
        .exports("ping.ts", &[])
        .site("ping.ts", "http GET /health", Role::Calls, 2)
        .build();

    let answer = ask(&index, "api.ts", &["listUsers"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("users.ts", 1, "calls http GET /users", &["fetchUsers"])]
    );
}

#[test]
fn a_namespace_import_is_a_use_of_every_name_by_what_uses_the_namespace() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_as("b.ts", "a.ts", &[("*", "ns")])
        .symbols(
            "b.ts",
            Code::new()
                .declaration("run", (3, 5), &["ns"])
                .declaration("idle", (7, 8), &[]),
        )
        .exports("b.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("b.ts", 1, "imports a.ts", &["run"])]
    );
}

#[test]
fn a_name_is_followed_under_the_local_name_an_import_gives_it() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_as("b.ts", "a.ts", &[("foo", "renamed")])
        .symbols(
            "b.ts",
            Code::new()
                .declaration("run", (3, 5), &["renamed"])
                .declaration("idle", (7, 8), &["foo"]),
        )
        .exports("b.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("b.ts", 1, "imports foo from a.ts", &["run"])]
    );
}

#[test]
fn a_default_export_is_followed_through_the_local_it_is_exported_from() {
    let index = Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .exports_from("default", "foo"),
        )
        .exports("a.ts", &["default"])
        .imports_as("b.ts", "a.ts", &[("default", "Thing")])
        .symbols("b.ts", Code::new().declaration("run", (3, 5), &["Thing"]))
        .exports("b.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("b.ts", 1, "imports default from a.ts", &["run"])]
    );
}

#[test]
fn a_name_exported_under_another_name_is_passed_on_by_the_file_that_imports_it() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("mid.ts", "a.ts", &["foo"])
        .symbols("mid.ts", Code::new().exports_from("bar", "foo"))
        .exports("mid.ts", &["bar"])
        .imports_names("page.ts", "mid.ts", &["bar"])
        .symbols(
            "page.ts",
            Code::new().declaration("render", (1, 4), &["bar"]),
        )
        .exports("page.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            row("mid.ts", 1, "imports foo from a.ts", &[]),
            row("page.ts", 1, "imports bar from mid.ts", &["render"]),
        ]
    );
}

#[test]
fn a_star_barrel_passes_a_name_on_but_never_the_default() {
    let index = Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .declaration("thing", (4, 5), &[])
                .exports_from("default", "thing"),
        )
        .exports("a.ts", &["foo", "default"])
        .re_exports_all("index.ts", "a.ts")
        .symbols("index.ts", Code::new())
        .exports("index.ts", &[])
        .imports_names("page.ts", "index.ts", &["foo", "default"])
        .symbols(
            "page.ts",
            Code::new()
                .declaration("render", (1, 4), &["foo"])
                .declaration("show", (6, 8), &["default"]),
        )
        .exports("page.ts", &[])
        .build();

    let foo = ask(&index, "a.ts", &["foo"], None);
    let thing = ask(&index, "a.ts", &["thing"], None);

    assert_eq!(
        rows(&foo.affected),
        [
            row("index.ts", 1, "re-exports foo from a.ts", &[]),
            row("page.ts", 1, "imports foo from index.ts", &["render"]),
        ]
    );
    assert!(thing.affected.is_empty(), "{:?}", thing.affected);
}

#[test]
fn an_import_nothing_uses_is_not_a_caller() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .symbols("b.ts", Code::new().declaration("run", (3, 5), &[]))
        .exports("b.ts", &["run"])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(paths(&answer.affected), NONE);
    assert!(
        answer
            .text(Entries::Capped)
            .contains("nothing is affected by a change to foo in a.ts among")
    );
}

#[test]
fn a_dynamic_import_is_a_use_by_the_declaration_that_holds_its_line_or_by_the_load() {
    let dynamic = |line| Import::new("a.ts", ImportKind::Dynamic, line);
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .import("inside.ts", dynamic(6), file("a.ts"))
        .symbols(
            "inside.ts",
            Code::new()
                .declaration("lazy", (5, 8), &[])
                .declaration("idle", (10, 12), &[]),
        )
        .exports("inside.ts", &[])
        .import("outside.ts", dynamic(20), file("a.ts"))
        .symbols("outside.ts", Code::new().declaration("lazy", (5, 8), &[]))
        .exports("outside.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            row("inside.ts", 1, "imports a.ts", &["lazy"]),
            (
                "outside.ts",
                1,
                "imports a.ts".to_owned(),
                Vec::new(),
                Some(1)
            ),
        ]
    );
}

#[test]
fn an_import_type_uses_the_names_it_takes_in_the_declaration_that_holds_its_line() {
    let index = Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .declaration("bar", (3, 4), &[]),
        )
        .exports("a.ts", &["foo", "bar"])
        .imports_type("named.ts", "a.ts", &["foo"], 6)
        .symbols(
            "named.ts",
            Code::new()
                .declaration("Alias", (5, 8), &[])
                .declaration("Idle", (10, 12), &[]),
        )
        .exports("named.ts", &[])
        .imports_type("module.ts", "a.ts", &["*"], 6)
        .symbols("module.ts", Code::new().declaration("Shape", (5, 8), &[]))
        .exports("module.ts", &[])
        .build();

    let foo = ask(&index, "a.ts", &["foo"], None);
    let bar = ask(&index, "a.ts", &["bar"], None);

    assert_eq!(
        rows(&foo.affected),
        [
            row("module.ts", 1, "imports a.ts", &["Shape"]),
            row("named.ts", 1, "imports foo from a.ts", &["Alias"]),
        ]
    );
    // `import('./a').foo` takes `foo` and nothing else, and `typeof import('./a')` takes every name.
    assert_eq!(
        rows(&bar.affected),
        [row("module.ts", 1, "imports a.ts", &["Shape"])]
    );
}

#[scenario("queries", "A load takes the names read from it")]
#[test]
fn a_load_takes_the_names_read_from_it() {
    let load = |names: &[&str], line| Import {
        names: names.iter().map(|&name| name.to_owned()).collect(),
        ..Import::new("a.ts", ImportKind::Dynamic, line)
    };
    let index = Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .declaration("bar", (4, 5), &[]),
        )
        .exports("a.ts", &["foo", "bar"])
        .import("b.ts", load(&["bar"], 2), file("a.ts"))
        .symbols("b.ts", Code::new().declaration("load", (1, 3), &[]))
        .exports("b.ts", &[])
        .import("c.ts", load(&["foo"], 1), file("a.ts"))
        .symbols("c.ts", Code::new())
        .exports("c.ts", &[])
        .import("d.ts", load(&["*"], 2), file("a.ts"))
        .symbols("d.ts", Code::new().declaration("start", (1, 3), &[]))
        .exports("d.ts", &[])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            (
                "c.ts",
                1,
                "imports foo from a.ts".to_owned(),
                Vec::new(),
                Some(1)
            ),
            row("d.ts", 1, "imports a.ts", &["start"]),
        ]
    );
}

#[scenario("queries", "An import type outside every declaration reaches nothing")]
#[test]
fn an_import_type_outside_every_declaration_reaches_nothing() {
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .import(
            "a.test.ts",
            Import::new("a.ts", ImportKind::Mock, 1),
            file("a.ts"),
        )
        .imports_type("a.test.ts", "a.ts", &["*"], 2)
        .symbols("a.test.ts", Code::new())
        .exports("a.test.ts", &[])
        .tag("a.test.ts", FileTags::TEST)
        .imports_type("types.ts", "a.ts", &["*"], 1)
        .symbols("types.ts", Code::new().declaration("Api", (1, 1), &[]))
        .exports("types.ts", &["Api"])
        .build();

    let answer = ask(&index, "a.ts", &["foo"], None);

    assert_eq!(
        rows(&answer.affected),
        [row("types.ts", 1, "imports a.ts", &["Api"])]
    );
    assert_eq!(paths(&answer.tests), NONE);
}

#[test]
fn a_side_effect_import_uses_no_name_but_loads_the_file() {
    let index = Tree::new()
        .symbols(
            "a.ts",
            Code::new()
                .declaration("foo", (1, 2), &[])
                .on_load(&["foo"]),
        )
        .exports("a.ts", &["foo"])
        .imports_for_effect("b.ts", "a.ts")
        .symbols("b.ts", Code::new().declaration("run", (3, 5), &["foo"]))
        .exports("b.ts", &[])
        .build();

    let by_name = ask(&index, "a.ts", &["foo"], Some(2));

    assert_eq!(
        rows(&by_name.affected),
        [
            ("a.ts", 1, "declares foo".to_owned(), Vec::new(), Some(1)),
            ("b.ts", 2, "imports a.ts".to_owned(), Vec::new(), Some(2)),
        ]
    );
}

/// `routes.ts` serves `/inner` in its handler and `/top` outside every declaration, `other.ts` calls `/inner` outside every declaration and `client.ts` calls `/top` in a function.
fn routes() -> Tree {
    Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("routes.ts", "a.ts", &["foo"])
        .symbols(
            "routes.ts",
            Code::new()
                .declaration("handler", (3, 8), &[])
                .on_load(&["foo"]),
        )
        .exports("routes.ts", &[])
        .site("routes.ts", "http GET /top", Role::Serves, 20)
        .site("routes.ts", "http GET /inner", Role::Serves, 5)
        .symbols(
            "client.ts",
            Code::new().declaration("fetchTop", (1, 4), &[]),
        )
        .exports("client.ts", &[])
        .site("client.ts", "http GET /top", Role::Calls, 2)
        .symbols(
            "other.ts",
            Code::new().declaration("fetchInner", (1, 4), &[]),
        )
        .exports("other.ts", &[])
        .site("other.ts", "http GET /inner", Role::Calls, 30)
}

#[test]
fn a_site_outside_every_declaration_belongs_to_the_load() {
    let index = routes().build();

    let answer = ask(&index, "routes.ts", &["handler"], None);

    assert_eq!(
        rows(&answer.affected),
        [(
            "other.ts",
            1,
            "calls http GET /inner".to_owned(),
            Vec::new(),
            Some(1)
        )]
    );
}

#[test]
fn a_file_reached_at_load_crosses_all_its_contracts() {
    let index = routes().build();

    let answer = ask(&index, "a.ts", &["foo"], Some(2));

    assert_eq!(
        rows(&answer.affected),
        [
            (
                "routes.ts",
                1,
                "imports foo from a.ts".to_owned(),
                Vec::new(),
                Some(1)
            ),
            (
                "client.ts",
                2,
                "calls http GET /top".to_owned(),
                vec!["fetchTop"],
                None
            ),
            (
                "other.ts",
                2,
                "calls http GET /inner".to_owned(),
                Vec::new(),
                Some(2)
            ),
        ]
    );
}

#[test]
fn several_names_in_several_files_are_one_question() {
    let index = library()
        .symbols("z.ts", Code::new().declaration("zed", (1, 2), &[]))
        .exports("z.ts", &["zed"])
        .imports_names("y.ts", "z.ts", &["zed"])
        .symbols("y.ts", Code::new().declaration("use", (1, 3), &["zed"]))
        .exports("y.ts", &[])
        .build();

    let answer = ask_in(&index, &["a.ts", "z.ts"], &["foo", "zed", "nothing"], None);

    assert_eq!(
        rows(&answer.affected),
        [
            row("b.ts", 1, "imports foo from a.ts", &["run"]),
            row("y.ts", 1, "imports zed from z.ts", &["use"]),
        ]
    );
    // Names apply to every path of the request, so each path that lacks one gets a note for it.
    assert_eq!(
        notes(&answer),
        [
            "a.ts declares no nothing",
            "a.ts declares no zed",
            "z.ts declares no foo",
            "z.ts declares no nothing"
        ]
    );
    assert!(
        answer
            .text(Entries::Capped)
            .starts_with("impact of foo, zed and nothing in a.ts and z.ts, within 1 hop\n")
    );
}

#[test]
fn the_callers_of_an_entry_are_cut_after_three_unless_all_are_asked_for() {
    let mut users = Code::new();
    for (number, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
        let line = u32::try_from(number).unwrap() * 3 + 1;
        users = users.declaration(name, (line, line + 1), &["foo"]);
    }
    let index = Tree::new()
        .symbols("lib.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("lib.ts", &["foo"])
        .imports_names("many.ts", "lib.ts", &["foo"])
        .symbols("many.ts", users)
        .exports("many.ts", &[])
        .build();

    let answer = ask(&index, "lib.ts", &["foo"], None);

    assert!(
        answer.text(Entries::Capped).contains(
            "many.ts  distance 1  imports foo from lib.ts; in a (line 1), b (line 4), c (line 7) and 2 more\n"
        ),
        "{}",
        answer.text(Entries::Capped)
    );
    assert!(
        answer.text(Entries::All).contains(
            "imports foo from lib.ts; in a (line 1), b (line 4), c (line 7), d (line 10), e (line 13)\n"
        ),
        "{}",
        answer.text(Entries::All)
    );
    assert_eq!(answer.affected[0].callers.len(), 5);
}

#[test]
fn an_answer_without_names_serialises_as_before() {
    let index = Tree::new().imports("b.ts", "a.ts").build();
    let request = ImpactRequest {
        files: vec!["a.ts".to_owned()],
        ..ImpactRequest::default()
    };

    let json = to_value(impact(&index, Freshness::Fresh, &request)).unwrap();

    assert!(json.get("names").is_none() && json.get("notes").is_none());
    assert_eq!(
        json["affected"],
        json!([{ "path": "b.ts", "distance": 1, "reason": { "via": "imports", "file": "a.ts" } }])
    );
}

#[test]
fn a_depth_of_zero_lists_nothing_and_says_the_walk_stopped() {
    let answer = ask(&chain(), "a.ts", &["foo"], Some(0));

    assert_eq!(paths(&answer.affected), NONE);
    assert_eq!(notes(&answer), [STOPPED]);
    assert!(
        answer
            .text(Entries::Capped)
            .starts_with("impact of foo in a.ts, within 0 hops\n")
    );
}

#[test]
fn a_name_asked_twice_is_one_name() {
    let answer = ask(&chain(), "a.ts", &["foo", "foo"], None);

    assert_eq!(answer.names, ["foo"]);
}

#[test]
fn an_index_read_from_a_snapshot_answers_by_name_as_it_did() {
    let tree = || {
        Tree::new()
            .symbols(
                "a.ts",
                Code::new()
                    .declaration("foo", (1, 2), &[])
                    .declaration("helper", (4, 6), &["foo"])
                    .exports_from("default", "helper"),
            )
            .exports("a.ts", &["foo", "helper", "default"])
            .imports_names("b.ts", "a.ts", &["foo"])
            .symbols(
                "b.ts",
                Code::new()
                    .declaration("run", (3, 5), &["foo"])
                    .on_load(&["foo"]),
            )
            .exports("b.ts", &["run"])
            .imports_names("c.ts", "b.ts", &["run"])
            .symbols("c.ts", Code::new().declaration("main", (1, 3), &["run"]))
            .exports("c.ts", &[])
            .build()
    };
    let before = to_value(ask(&tree(), "a.ts", &["foo"], Some(3))).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot.gsttt");
    let header = Header {
        built_by: VERSION.to_owned(),
        built_at: now(),
        head: None,
        dirty: 0,
        files: 0,
        connectors: Vec::new(),
        coverage: String::new(),
    };
    let stored = Stored {
        index: tree(),
        config: 0,
        listed_at: 0,
        files: Vec::new(),
    };
    write(&path, &header, &stored).unwrap();

    let snapshot = Snapshot::open(&path).unwrap();
    let read = snapshot.stored().unwrap().to_index().unwrap();
    let after = to_value(ask(&read, "a.ts", &["foo"], Some(3))).unwrap();

    assert_eq!(after, before);
    assert_eq!(
        before["affected"][1]["callers"],
        json!([{ "name": "run", "line": 3, "distance": 1 }])
    );
    assert_eq!(before["affected"][1]["load"], json!({ "distance": 1 }));
}

/// How many files a random tree has.
const FILES: usize = 5;

/// The names the declarations of a random tree can have.
const NAMES: [&str; 3] = ["a", "b", "c"];

/// What one file of a random tree declares, uses, exports and imports.
#[derive(Clone, Debug)]
struct Module {
    /// Per name: whether the file declares it, and a mask of the names that declaration uses.
    declared: [Option<u8>; 3],
    /// A mask of the names the code of the file uses as it loads.
    on_load: u8,
    /// A mask of the names the file exports, of which it keeps those it declares.
    exports: u8,
    /// Per file: a mask of the names this file imports from it, none when zero.
    imports: [u8; FILES],
}

fn module() -> impl Strategy<Value = Module> {
    (
        prop::array::uniform3(prop::option::of(0_u8..8)),
        0_u8..8,
        0_u8..8,
        prop::array::uniform5(0_u8..8),
    )
        .prop_map(|(declared, on_load, exports, imports)| Module {
            declared,
            on_load,
            exports,
            imports,
        })
}

/// The names `mask` picks out of [`NAMES`].
fn picked(mask: u8) -> Vec<&'static str> {
    NAMES
        .into_iter()
        .enumerate()
        .filter(|&(at, _)| mask & (1 << at) != 0)
        .map(|(_, name)| name)
        .collect()
}

fn module_path(number: usize) -> String {
    format!("f{number}.ts")
}

/// The index of the files of `modules`, each with declarations of its own.
fn random_tree(modules: &[Module]) -> Index {
    let mut tree = Tree::new();
    for (number, module) in modules.iter().enumerate() {
        let path = module_path(number);
        let mut code = Code::new().on_load(&picked(module.on_load));
        let mut exported = 0_u8;
        for (at, declared) in module.declared.iter().enumerate() {
            let (Some(uses), Ok(line)) = (declared, u32::try_from(at * 3 + 1)) else {
                continue;
            };
            code = code.declaration(NAMES[at], (line, line + 1), &picked(*uses));
            exported |= module.exports & (1 << at);
        }
        tree = tree.symbols(&path, code).exports(&path, &picked(exported));
        for (target, &mask) in module.imports.iter().enumerate() {
            if mask != 0 && target != number {
                tree = tree.imports_names(&path, &module_path(target), &picked(mask));
            }
        }
    }
    tree.build()
}

/// What a reader of the answer sees of each entry: its path with its distance, its callers each with its distance, and the distance of its load when it is reached.
type Seen = BTreeMap<String, (u32, BTreeMap<String, u32>, Option<u32>)>;

fn seen(answer: &Impact) -> Seen {
    answer
        .affected
        .iter()
        .chain(&answer.tests)
        .map(|entry| {
            let callers = entry
                .callers
                .iter()
                .map(|caller| (caller.name.clone(), caller.distance));
            (
                entry.path.clone(),
                (
                    entry.distance,
                    callers.collect(),
                    entry.load.as_ref().map(|load| load.distance),
                ),
            )
        })
        .collect()
}

proptest! {
    /// On any tree, a larger depth keeps what a smaller one found, each file, caller and load at the same distance, lists nothing beyond the depth, lists nearest first, puts no caller or load nearer than the entry that holds it, and gives the same answer twice.
    #[test]
    fn a_larger_depth_keeps_what_a_smaller_one_found(modules in prop::collection::vec(module(), FILES)) {
        let index = random_tree(&modules);
        for number in 0..FILES {
            for name in NAMES {
                let path = module_path(number);
                let mut nearer: Seen = Seen::new();
                for depth in 0..=4 {
                    let answer = ask(&index, &path, &[name], Some(depth));
                    prop_assert_eq!(
                        to_value(&answer).unwrap(),
                        to_value(ask(&index, &path, &[name], Some(depth))).unwrap()
                    );
                    for list in [&answer.affected, &answer.tests] {
                        let order: Vec<(u32, &str)> = list
                            .iter()
                            .map(|entry| (entry.distance, entry.path.as_str()))
                            .collect();
                        prop_assert!(order.is_sorted(), "{order:?}");
                        prop_assert!(order.iter().all(|&(distance, _)| (1..=depth).contains(&distance)), "{order:?}");
                        for entry in list {
                            let loads = entry.load.iter().map(|load| load.distance);
                            let hops = entry.callers.iter().map(|caller| caller.distance).chain(loads);
                            for hop in hops {
                                prop_assert!((entry.distance..=depth).contains(&hop), "{} at {} holds a part at {hop}", entry.path, entry.distance);
                            }
                        }
                    }
                    let now = seen(&answer);
                    for (file, (distance, callers, load)) in &nearer {
                        let (later, more, loaded) = &now[file];
                        prop_assert_eq!(later, distance);
                        for (caller, hops) in callers {
                            prop_assert_eq!(more.get(caller), Some(hops), "{} in {}", caller, file);
                        }
                        prop_assert!(load.is_none() || loaded == load, "{file}");
                    }
                    nearer = now;
                }
            }
        }
    }
}

/// What the code of the files of a generated tree refers to.
#[derive(Clone, Copy)]
enum Uses {
    /// The declarations of a file refer to some of what the file declares and imports, and no code runs as the file loads.
    InDeclarations,
    /// As [`Uses::InDeclarations`], and the code that runs as the file loads refers to some of the same.
    AlsoAtLoad,
    /// Only the code that runs as the file loads refers to anything, and it refers to every import of the file.
    EveryImportAtLoad,
}

/// A generated tree, written the way a TypeScript connector records it.
struct Written {
    index: Index,
    /// The names the file that changes declares, which is what a walk by name asks about.
    asked: Vec<String>,
    /// The code of every file and what every import binds, for a failure to be read.
    shown: String,
}

/// The names `file` re-exports, which it exports without declaring them.
fn reexported(case: &Case, file: usize) -> BTreeSet<&'static str> {
    case.edges
        .iter()
        .filter(|&&(importer, target, _)| importer == file && target != file)
        .filter_map(|(_, _, take)| match take {
            Take::ReExport(pairs) => Some(pairs),
            _ => None,
        })
        .flatten()
        .map(|&(exported, _)| exported)
        .collect()
}

/// The files of `case` with the declarations, exports and imports a connector would record for them. `choices` settles what the case leaves open: which exported names are declared under another name, which imports bind a name under another, and what each declaration, and the code that runs as the file loads, refers to.
fn written(case: &Case, uses: Uses, choices: &[bool]) -> Written {
    let mut cycle = choices.iter().copied().cycle();
    let mut flip = move || cycle.next() == Some(true);
    let files = case.own.len();

    // Each name a file exports and declares, with the declaration that holds it: the name itself, or `<name>_impl` exported as `<name>`.
    let declared: Vec<Vec<(&str, String)>> = (0..files)
        .map(|file| {
            let reexported = reexported(case, file);
            case.own[file]
                .iter()
                .flatten()
                .filter(|&&name| !reexported.contains(name))
                .map(|&name| {
                    let declaration = if flip() {
                        format!("{name}_impl")
                    } else {
                        name.to_owned()
                    };
                    (name, declaration)
                })
                .collect()
        })
        .collect();

    // What each import binds, as the name it takes and the local it binds it to. A name keeps its own when nothing else in the file has it; `default` and a namespace cannot.
    let mut scope: Vec<BTreeSet<String>> = declared
        .iter()
        .map(|file| {
            file.iter()
                .map(|(_, declaration)| declaration.clone())
                .collect()
        })
        .collect();
    let bindings: Vec<Vec<(&str, String)>> = case
        .edges
        .iter()
        .enumerate()
        .map(|(edge, (importer, _, take))| {
            let taken: &[&str] = match take {
                Take::Named(names) => names,
                Take::Namespace => &["*"],
                Take::Plain | Take::ReExport(_) | Take::Star => &[],
            };
            taken
                .iter()
                .enumerate()
                .map(|(at, &name)| {
                    let keeps = name != "default"
                        && name != "*"
                        && !flip()
                        && scope[*importer].insert(name.to_owned());
                    let local = if keeps {
                        name.to_owned()
                    } else {
                        format!("{}_{edge}_{at}", name.replace('*', "ns"))
                    };
                    scope[*importer].insert(local.clone());
                    (name, local)
                })
                .collect()
        })
        .collect();

    let mut shown = String::new();
    let mut tree = Tree::new();
    for (file, (declares, own)) in declared.iter().zip(&case.own).enumerate() {
        let path = file_path(file);
        tree = tree.read_by("ts", &path);
        let Some(own) = own else {
            writeln!(
                shown,
                "{path} records no declarations and does not list its exports"
            )
            .unwrap();
            continue;
        };
        let locals: Vec<&str> = case
            .edges
            .iter()
            .zip(&bindings)
            .filter(|((importer, _, _), _)| *importer == file)
            .flat_map(|(_, bound)| bound.iter().map(|(_, local)| local.as_str()))
            .collect();
        let names: Vec<&str> = declares
            .iter()
            .map(|(_, declaration)| declaration.as_str())
            .chain(locals.iter().copied())
            .collect();
        let mut code = Code::new();
        for (at, (exported, declaration)) in declares.iter().enumerate() {
            let refers: Vec<&str> = match uses {
                Uses::EveryImportAtLoad => Vec::new(),
                Uses::InDeclarations | Uses::AlsoAtLoad => names
                    .iter()
                    .copied()
                    .filter(|&other| other != declaration && flip())
                    .collect(),
            };
            let line = 10 + 3 * u32::try_from(at).unwrap();
            code = code.declaration(declaration, (line, line + 1), &refers);
            if declaration != exported {
                code = code.exports_from(exported, declaration);
            }
        }
        code = match uses {
            Uses::InDeclarations => code,
            Uses::AlsoAtLoad => {
                let loads: Vec<&str> = names.iter().copied().filter(|_| flip()).collect();
                code.on_load(&loads)
            }
            Uses::EveryImportAtLoad => code.on_load(&locals),
        };
        let exports: Vec<&str> = own.iter().copied().collect();
        writeln!(shown, "{path} exports {exports:?}: {code:?}").unwrap();
        tree = tree.symbols(&path, code).exports(&path, &exports);
    }
    for ((importer, target, take), bound) in case.edges.iter().zip(&bindings) {
        let (from, to) = (file_path(*importer), file_path(*target));
        writeln!(shown, "{from} -> {to}: {take:?} binds {bound:?}").unwrap();
        let binds: Vec<(&str, &str)> = bound
            .iter()
            .map(|(name, local)| (*name, local.as_str()))
            .collect();
        tree = match take {
            Take::Plain => tree.imports(&from, &to),
            // A file whose connector records no declarations records the names an import takes and binds none.
            Take::Named(_) | Take::Namespace if case.own[*importer].is_none() => {
                let taken: Vec<&str> = binds.iter().map(|&(name, _)| name).collect();
                tree.imports_names(&from, &to, &taken)
            }
            Take::Named(_) | Take::Namespace => tree.imports_as(&from, &to, &binds),
            Take::ReExport(pairs) => tree.re_exports(&from, &to, pairs),
            Take::Star => tree.re_exports_all(&from, &to),
        };
    }
    let asked = declared[case.changed]
        .iter()
        .map(|(_, declaration)| declaration.clone())
        .collect();
    Written {
        index: tree.build(),
        asked,
        shown,
    }
}

/// Every file an answer lists.
fn listed(answer: &Impact) -> BTreeSet<String> {
    answer
        .affected
        .iter()
        .chain(&answer.tests)
        .map(|entry| entry.path.clone())
        .collect()
}

/// The files that `by_name` lists when asked about `names` of `changed`, with no limit on the depth, and that `per_file` does not list when asked about `changed`. The file itself is left out, because the walk by name lists it when its own code refers to the names.
fn only_by_name(by_name: &Index, per_file: &Index, changed: &str, names: &[String]) -> Vec<String> {
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut by_name = listed(&ask(by_name, changed, &names, Some(u32::MAX)));
    by_name.remove(changed);
    let per_file = listed(&ask(per_file, changed, &NONE, None));
    by_name.difference(&per_file).cloned().collect()
}

/// The names an `unknown` file can be asked about: it declares none that a connector would record, so the walk follows it whole whatever it is asked.
fn names_to_ask(asked: &[String]) -> Vec<String> {
    if asked.is_empty() {
        vec!["a".to_owned()]
    } else {
        asked.to_vec()
    }
}

/// The tree of `case` as a connector that records declarations reads it. Such a connector records the names every import takes, so no file with declarations has a plain import. The walk by name follows a plain import whole, as it follows a file reached at load, which reaches every file that loads it through the barrels in between, where the walk per file passes on only the names a barrel can (design.md, "The walk by name").
fn recorded(mut case: Case) -> Case {
    let own = case.own.clone();
    case.edges
        .retain(|(importer, _, take)| !matches!(take, Take::Plain) || own[*importer].is_none());
    case
}

/// The tree of `case` as the next property needs it, where the two walks list the same files. What the design makes the two walks do differently is taken out of it:
///
/// - A barrel that lists its exports passes a name on narrowly, where a file reached at load reaches every file that loads it, whatever name that file takes (design.md, "The walk by name"). So a file that re-exports lists no exports and records no declarations, and the walk by name follows it whole. The file that changes is not one, because it has declarations to ask about.
/// - The walk per file reaches an importer of a name its target does not export, because the import may be what the change breaks. The walk by name has no name to follow, so every name that is imported is exported.
/// - A star reaches its importer even when the file has nothing to pass on, and `export *` never forwards `default`, so a file with a star importer exports a name besides `default`.
/// - A side-effect import uses nothing by name, so a file reached by name does not reach the file that imports it for its effects, where the walk per file does (design.md, "A side-effect import uses nothing by name"). `Take` has no such import, and it must not get one while this property takes its trees from it.
fn complete(mut case: Case) -> Case {
    let own = case.own.clone();
    case.edges.retain(|(importer, _, take)| {
        !matches!(take, Take::ReExport(_) | Take::Star) || own[*importer].is_none()
    });
    let exports = exported(&case);
    for (_, target, take) in &case.edges {
        let taken: Vec<&'static str> = match take {
            Take::Named(names) => names.clone(),
            Take::ReExport(pairs) => pairs
                .iter()
                .map(|&(_, source)| source)
                .filter(|&source| source != "*")
                .collect(),
            Take::Plain | Take::Namespace | Take::Star => Vec::new(),
        };
        // A file whose exports are unknown cannot be told to lack a name.
        if let (Some(own), Some(known)) = (&mut case.own[*target], &exports[*target]) {
            own.extend(taken.into_iter().filter(|name| !known.contains(name)));
        }
    }
    let changed = case.changed;
    if !case.own[changed]
        .iter()
        .flatten()
        .any(|&name| name != "default")
    {
        case.edges
            .retain(|(_, target, take)| *target != changed || !matches!(take, Take::Star));
    }
    case
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    /// On a tree where names narrow what a barrel passes on, every import of a file with declarations says what it takes, and no code runs as a file loads, a walk by name asked about names of the changed file with no limit on the depth lists no file that the walk per file does not list.
    #[test]
    fn a_walk_by_name_lists_no_file_the_walk_per_file_does_not(
        case in case(),
        choices in prop::collection::vec(any::<bool>(), 256),
    ) {
        let case = recorded(case);
        let written = written(&case, Uses::InDeclarations, &choices);
        let extra = only_by_name(
            &written.index,
            &written.index,
            &file_path(case.changed),
            &names_to_ask(&written.asked),
        );
        prop_assert!(extra.is_empty(), "listed by name and not per file: {extra:?}\n{case:?}\n{}", written.shown);
    }

    /// With code that runs as a file loads, a walk by name lists no file that the walk over files with every name taken out does not list: it follows imports, and only imports, however it narrows them. It may list a file the walk per file leaves out, because a file reached at load reaches every file that loads it whatever name that file takes, where the walk per file stops at a barrel that lists what it exports.
    #[test]
    fn a_walk_by_name_lists_no_file_a_walk_over_whole_files_does_not(
        case in case(),
        choices in prop::collection::vec(any::<bool>(), 256),
    ) {
        let written = written(&case, Uses::AlsoAtLoad, &choices);
        let extra = only_by_name(
            &written.index,
            &case.by_file(),
            &file_path(case.changed),
            &names_to_ask(&written.asked),
        );
        prop_assert!(extra.is_empty(), "listed by name and not over whole files: {extra:?}\n{case:?}\n{}", written.shown);
    }

    /// When every file uses, as it loads, every name it imports, a file that is reached is reached at load, and a file reached at load reaches every file that loads it. A walk by name asked about every name the changed file declares, with no limit on the depth, then lists the files the walk per file lists. The changed file is left out, because the walk by name lists it when a cycle leads back to it.
    #[test]
    fn a_walk_by_every_name_lists_what_the_walk_per_file_does_when_every_import_is_used(
        case in case(),
        choices in prop::collection::vec(any::<bool>(), 256),
    ) {
        let case = complete(case);
        let written = written(&case, Uses::EveryImportAtLoad, &choices);
        if written.asked.is_empty() {
            return Ok(());
        }
        let changed = file_path(case.changed);
        let names: Vec<&str> = written.asked.iter().map(String::as_str).collect();
        let mut by_name = listed(&ask(&written.index, &changed, &names, Some(u32::MAX)));
        by_name.remove(&changed);
        let per_file = listed(&ask(&written.index, &changed, &NONE, None));
        prop_assert_eq!(by_name, per_file, "\n{:?}\n{}", case, written.shown);
    }
}
