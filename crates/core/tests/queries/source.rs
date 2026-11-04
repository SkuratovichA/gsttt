//! The lines an `impact` answer shows beside the files nearest to the change, and the generated files it counts apart.

use std::cell::Cell;

use gsttt_core::fact::FileTags;
use gsttt_core::index::{Freshness, Index};
use gsttt_core::query::{Affected, Answer, Entries, Impact, ImpactRequest, LIST_CAP, impact};
use gsttt_core::testing::{Code, Tree};
use gsttt_scenario::scenario;
use serde_json::{json, to_value};

/// `b.ts` as it is on disk when the index was built: `run` starts on line 2 and calls `foo` on line 3.
const B: &str = "import { foo } from './a';\nexport function run() {\n  return foo();\n}\n";

/// `a.ts` declares `foo`, `b.ts` imports it and uses it in `run`, which starts at line 2 and uses it on line 3.
fn called_from_b() -> Index {
    Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .symbols(
            "b.ts",
            Code::new().declaration_at("run", (2, 4), &[("foo", 3)]),
        )
        .exports("b.ts", &[])
        .build()
}

fn request(names: &[&str]) -> ImpactRequest {
    ImpactRequest {
        files: vec!["a.ts".to_owned()],
        names: names.iter().map(|&name| name.to_owned()).collect(),
        ..ImpactRequest::default()
    }
}

/// The answer about `foo` in `a.ts`, then the lines `read` gives, which gets each path asked for.
fn shown(index: &Index, freshness: Freshness, read: impl FnMut(&str) -> Option<String>) -> Impact {
    let mut answer = impact(index, freshness, &request(&["foo"]));
    answer.show_source(read);
    answer
}

fn only_b(path: &str) -> Option<String> {
    (path == "b.ts").then(|| B.to_owned())
}

fn entry<'a>(answer: &'a Impact, path: &str) -> &'a Affected {
    answer
        .affected
        .iter()
        .find(|entry| entry.path == path)
        .unwrap_or_else(|| panic!("{path} is not listed in {:?}", answer.affected))
}

#[scenario(
    "queries",
    "A caller at distance 1 shows the line of its call and its text"
)]
#[test]
fn a_caller_at_distance_1_shows_the_line_of_its_call_and_its_text() {
    let answer = shown(&called_from_b(), Freshness::Fresh, only_b);

    let text = answer.text(Entries::Capped);
    assert!(
        text.contains(
            "\nb.ts:3  distance 1  imports foo from a.ts; in run (line 2)\n> return foo();\n"
        ),
        "{text}"
    );
    let json = to_value(&answer).unwrap();
    assert_eq!(json["affected"][0]["at"], json!(3));
    assert_eq!(json["affected"][0]["snippet"], json!("return foo();"));
}

#[scenario(
    "queries",
    "An import with no use of its own shows the line of the import"
)]
#[test]
fn an_import_with_no_use_of_its_own_shows_the_line_of_the_import() {
    // The connector records no declarations for `b.ts`, so the walk follows it whole and no use of its own made it a caller.
    let index = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"])
        .imports_names("b.ts", "a.ts", &["foo"])
        .build();

    let answer = shown(&index, Freshness::Fresh, only_b);

    let text = answer.text(Entries::Capped);
    assert!(
        text.contains(
            "\nb.ts:1  distance 1  imports foo from a.ts\n> import { foo } from './a';\n"
        ),
        "{text}"
    );
    assert_eq!(entry(&answer, "b.ts").at, Some(1));
}

#[scenario("queries", "A line the file has moved away from is not shown")]
#[test]
fn a_line_the_file_has_moved_away_from_is_not_shown() {
    // The same file after an edit above `run`: line 3 no longer calls `foo`.
    let moved = "import { foo } from './a';\nexport function run() {\n  return bar();\n}\n";

    let answer = shown(&called_from_b(), Freshness::Fresh, |path| {
        (path == "b.ts").then(|| moved.to_owned())
    });

    let kept = entry(&answer, "b.ts");
    assert_eq!(kept.callers.len(), 1);
    assert_eq!((kept.at, kept.snippet.as_deref()), (None, None));
    let text = answer.text(Entries::Capped);
    assert!(
        text.contains("\nb.ts  distance 1  imports foo from a.ts; in run (line 2)\n"),
        "{text}"
    );
    assert!(!text.contains("> "), "{text}");
    let json = to_value(&answer).unwrap();
    assert!(json["affected"][0].get("at").is_none(), "{json}");
    assert!(json["affected"][0].get("snippet").is_none(), "{json}");

    // A file that is not there to read is the same as one that moved on.
    let gone = shown(&called_from_b(), Freshness::Fresh, |_| None);
    assert_eq!(entry(&gone, "b.ts").at, None);
}

#[scenario(
    "queries",
    "Only a fresh answer shows lines, and only nearest files within the first thirty"
)]
#[test]
fn only_a_fresh_answer_shows_lines_and_only_nearest_files_within_the_first_thirty() {
    let mut tree = Tree::new()
        .symbols("a.ts", Code::new().declaration("foo", (1, 2), &[]))
        .exports("a.ts", &["foo"]);
    let importers: Vec<String> = (0..LIST_CAP + 2).map(|i| format!("f{i:02}.ts")).collect();
    for path in &importers {
        tree = tree
            .imports_names(path, "a.ts", &["foo"])
            .symbols(
                path,
                Code::new().declaration_at("run", (1, 3), &[("foo", 2)]),
            )
            .exports(path, &[]);
    }
    // `c.ts` calls `f00.ts`, which is two hops from `a.ts`.
    let index = tree
        .imports_names("c.ts", "f00.ts", &["run"])
        .symbols(
            "c.ts",
            Code::new().declaration_at("go", (1, 3), &[("run", 2)]),
        )
        .exports("c.ts", &[])
        .exports("f00.ts", &["run"])
        .build();
    let source = "function run() {\n  foo();\n}\nrun();\n";
    let asked = Cell::new(0);
    let read = |_: &str| {
        asked.set(asked.get() + 1);
        Some(source.to_owned())
    };

    let mut deep = impact(
        &index,
        Freshness::Fresh,
        &ImpactRequest {
            depth: Some(2),
            ..request(&["foo"])
        },
    );
    deep.show_source(read);

    assert_eq!(asked.get(), LIST_CAP, "the files read");
    let with_line = deep.affected.iter().filter(|entry| entry.at.is_some());
    assert_eq!(with_line.count(), LIST_CAP);
    assert_eq!(entry(&deep, "c.ts").distance, 2);
    assert_eq!(entry(&deep, "c.ts").at, None);

    let stale = Freshness::Stale {
        added: 0,
        removed: 0,
        changed: 1,
    };
    asked.set(0);
    let mut old = impact(&index, stale, &request(&["foo"]));
    old.show_source(|_| {
        asked.set(asked.get() + 1);
        Some(source.to_owned())
    });
    assert_eq!(asked.get(), 0, "a stale index reads no file");
    assert!(old.affected.iter().all(|entry| entry.at.is_none()));
}

#[scenario("queries", "A long line is cut around the name")]
#[test]
fn a_long_line_is_cut_around_the_name() {
    let long = format!(
        "import {{ foo }} from './a';\nexport function run() {{\n  {}foo(){}\n}}\n",
        "x".repeat(299),
        "y".repeat(199)
    );

    let answer = shown(&called_from_b(), Freshness::Fresh, |_| Some(long.clone()));

    let snippet = entry(&answer, "b.ts").snippet.clone().unwrap();
    assert!(
        snippet.chars().count() <= 160,
        "{}",
        snippet.chars().count()
    );
    assert!(snippet.contains("foo()"), "{snippet}");
    assert!(
        snippet.starts_with('…') && snippet.ends_with('…'),
        "{snippet}"
    );
}

/// `a.ts` is imported by `b.ts`, by the generated `gen.ts` and by `a.test.ts`, a test that is generated too.
fn with_generated() -> Index {
    Tree::new()
        .imports("b.ts", "a.ts")
        .imports("gen.ts", "a.ts")
        .imports("a.test.ts", "a.ts")
        .tag("gen.ts", FileTags::GENERATED)
        .tag("a.test.ts", FileTags::TEST | FileTags::GENERATED)
        .build()
}

fn paths(list: &[Affected]) -> Vec<&str> {
    list.iter().map(|entry| entry.path.as_str()).collect()
}

#[scenario("queries", "Generated files are counted and not listed")]
#[test]
fn generated_files_are_counted_and_not_listed() {
    let answer = impact(
        &with_generated(),
        Freshness::Fresh,
        &ImpactRequest {
            no_generated: true,
            generated_hint: Some("ask with generated=true"),
            ..request(&[])
        },
    );

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert_eq!(answer.generated.len(), 0);
    assert_eq!(answer.generated_left_out, 1);
    let text = answer.text(Entries::Capped);
    assert!(
        text.contains("\ngenerated: 1 file (ask with generated=true)\n"),
        "{text}"
    );
    let json = to_value(&answer).unwrap();
    assert_eq!(json["generated_left_out"], json!(1));
    assert!(
        !json.to_string().contains("ask with generated=true"),
        "{json}"
    );
}

#[scenario("queries", "Generated files are listed when asked")]
#[test]
fn generated_files_are_listed_when_asked() {
    let answer = impact(&with_generated(), Freshness::Fresh, &request(&[]));

    assert_eq!(paths(&answer.affected), ["b.ts"]);
    assert_eq!(paths(&answer.generated), ["gen.ts"]);
    assert_eq!(answer.generated_left_out, 0);
    let text = answer.text(Entries::Capped);
    assert!(
        text.contains("\ngenerated files (1):\ngen.ts  distance 1  imports a.ts\n"),
        "{text}"
    );
    assert!(!text.contains("generated: "), "{text}");
    let json = to_value(&answer).unwrap();
    assert_eq!(json["generated"][0]["path"], json!("gen.ts"));
    assert!(json.get("generated_left_out").is_none(), "{json}");
}

#[scenario("queries", "A generated test stays with the tests")]
#[test]
fn a_generated_test_stays_with_the_tests() {
    let index = Tree::new()
        .imports("a.test.ts", "a.ts")
        .tag("a.test.ts", FileTags::TEST | FileTags::GENERATED)
        .build();

    let answer = impact(
        &index,
        Freshness::Fresh,
        &ImpactRequest {
            no_generated: true,
            ..request(&[])
        },
    );

    assert_eq!(paths(&answer.tests), ["a.test.ts"]);
    assert_eq!(answer.generated_left_out, 0);
    assert_eq!(answer.generated.len(), 0);
}
