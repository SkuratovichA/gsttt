//! `dependents` and `dependencies`: one level of the import graph around a file.

use gsttt_core::fact::{Import, ImportFlags, ImportKind, ParseFailure, UnresolvedReason};
use gsttt_core::index::Freshness;
use gsttt_core::query::{Answer, External, Neighbour, dependencies, dependents};
use gsttt_core::testing::{To, Tree, file, package};
use gsttt_scenario::scenario;

fn neighbour(path: &str, kinds: &[ImportKind], type_only: bool) -> Neighbour {
    Neighbour {
        path: path.to_owned(),
        kinds: kinds.to_vec(),
        type_only,
    }
}

fn external(name: &str, lines: &[u32]) -> External {
    External {
        name: name.to_owned(),
        lines: lines.to_vec(),
    }
}

#[scenario("queries", "Dependencies show every outcome")]
#[test]
fn dependencies_show_every_outcome() {
    let index = Tree::new()
        .import(
            "src/a.ts",
            Import::new("./b", ImportKind::Static, 1),
            file("src/b.ts"),
        )
        .import(
            "src/a.ts",
            Import::new("react", ImportKind::Static, 2),
            package("react"),
        )
        .import(
            "src/a.ts",
            Import::new("node:fs", ImportKind::Static, 3),
            To::Builtin,
        )
        .import(
            "src/a.ts",
            Import::new("./missing", ImportKind::Static, 4),
            To::Missing(UnresolvedReason::NotFound),
        )
        .build();

    let answer = dependencies(&index, Freshness::Fresh, "src/a.ts");

    assert_eq!(
        answer.files,
        [neighbour("src/b.ts", &[ImportKind::Static], false)]
    );
    assert_eq!(answer.packages, [external("react", &[2])]);
    assert_eq!(answer.builtins, [external("node:fs", &[3])]);
    let unresolved: Vec<(&str, u32, &str, String)> = answer
        .unresolved
        .iter()
        .map(|miss| {
            (
                miss.path.as_str(),
                miss.line,
                miss.spec.as_str(),
                miss.reason.to_string(),
            )
        })
        .collect();
    assert_eq!(
        unresolved,
        [("src/a.ts", 4, "./missing", "no such file".to_owned())]
    );

    let text = answer.to_string();
    for line in [
        "src/b.ts  static import\n",
        "react  src/a.ts:2\n",
        "node:fs  src/a.ts:3\n",
        "src/a.ts:4 imports ./missing (no such file)\n",
    ] {
        assert!(text.contains(line), "{line:?} is missing from\n{text}");
    }
}

#[scenario("queries", "Dependents show edge kinds")]
#[test]
fn dependents_show_edge_kinds() {
    let index = Tree::new()
        .import(
            "b.ts",
            Import::new("./a", ImportKind::Static, 1),
            file("a.ts"),
        )
        .import(
            "c.ts",
            Import::new("./a", ImportKind::ReExportAll, 1),
            file("a.ts"),
        )
        .build();

    let answer = dependents(&index, Freshness::Fresh, "a.ts");

    assert_eq!(
        answer.files,
        [
            neighbour("b.ts", &[ImportKind::Static], false),
            neighbour("c.ts", &[ImportKind::ReExportAll], false)
        ]
    );
    let text = answer.to_string();
    assert!(text.contains("b.ts  static import\n"), "{text}");
    assert!(text.contains("c.ts  re-export of everything\n"), "{text}");
}

#[test]
fn an_edge_made_of_several_imports_lists_each_kind() {
    let index = Tree::new()
        .import(
            "b.ts",
            Import::new("./a", ImportKind::Static, 1),
            file("a.ts"),
        )
        .import(
            "b.ts",
            Import::new("./a", ImportKind::Dynamic, 9),
            file("a.ts"),
        )
        .build();

    let text = dependents(&index, Freshness::Fresh, "a.ts").to_string();

    assert!(
        text.contains("b.ts  static import, dynamic import\n"),
        "{text}"
    );
}

#[test]
fn an_edge_that_exists_only_for_types_says_so() {
    let mut import = Import::new("./a", ImportKind::Static, 1);
    import.flags = ImportFlags::TYPE_ONLY;
    let index = Tree::new().import("b.ts", import, file("a.ts")).build();

    let answer = dependents(&index, Freshness::Fresh, "a.ts");

    assert_eq!(
        answer.files,
        [neighbour("b.ts", &[ImportKind::Static], true)]
    );
    assert!(
        answer
            .to_string()
            .contains("b.ts  static import (types only)\n")
    );
}

#[test]
fn a_mod_declaration_is_listed_as_what_it_is() {
    let index = Tree::new()
        .import(
            "src/lib.rs",
            Import::new("store", ImportKind::Module, 1),
            file("src/store.rs"),
        )
        .build();

    let text = dependents(&index, Freshness::Fresh, "src/store.rs").to_string();

    assert!(text.contains("src/lib.rs  mod declaration\n"), "{text}");
}

#[test]
fn an_import_that_resolves_to_several_files_lists_each_of_them() {
    let index = Tree::new()
        .import(
            "src/main.ts",
            Import::new("./routes/*", ImportKind::Static, 1),
            To::Files(vec![
                "src/routes/b.ts".to_owned(),
                "src/routes/a.ts".to_owned(),
            ]),
        )
        .build();

    let imported = dependencies(&index, Freshness::Fresh, "src/main.ts");
    let importers = dependents(&index, Freshness::Fresh, "src/routes/b.ts");

    assert_eq!(
        imported.files,
        [
            neighbour("src/routes/a.ts", &[ImportKind::Static], false),
            neighbour("src/routes/b.ts", &[ImportKind::Static], false)
        ]
    );
    assert_eq!(
        importers.files,
        [neighbour("src/main.ts", &[ImportKind::Static], false)]
    );
}

#[test]
fn dependencies_are_ordered_by_path_and_packages_by_name() {
    let index = Tree::new()
        .imports("a.ts", "z.ts")
        .imports("a.ts", "m.ts")
        .import(
            "a.ts",
            Import::new("vue", ImportKind::Static, 5),
            package("vue"),
        )
        .import(
            "a.ts",
            Import::new("react", ImportKind::Static, 7),
            package("react"),
        )
        .import(
            "a.ts",
            Import::new("react/jsx-runtime", ImportKind::Static, 2),
            package("react"),
        )
        .build();

    let answer = dependencies(&index, Freshness::Fresh, "a.ts");

    let paths: Vec<&str> = answer.files.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(paths, ["m.ts", "z.ts"]);
    assert_eq!(
        answer.packages,
        [external("react", &[2, 7]), external("vue", &[5])]
    );
}

/// An import that points at nothing, which is all the ordering tests below need of it.
fn missing(spec: &str, kind: ImportKind, line: u32) -> (Import, To) {
    (
        Import::new(spec, kind, line),
        To::Missing(UnresolvedReason::NotFound),
    )
}

#[test]
fn possible_misses_are_ordered_by_path_then_line_then_specifier() {
    // Each file lists its imports out of line order, as a connector that finds dynamic imports after static ones
    // would.
    let mut tree = Tree::new().read_by("ts", "src/old.ts");
    for (from, spec, kind, line) in [
        ("src/b.ts", "./old", ImportKind::Static, 9),
        ("src/b.ts", "./old", ImportKind::Dynamic, 2),
        ("src/a.ts", "./old", ImportKind::Static, 5),
        ("src/a.ts", "../lib/old", ImportKind::Static, 5),
    ] {
        let (import, to) = missing(spec, kind, line);
        tree = tree.import(from, import, to);
    }
    let index = tree.build();

    let answer = dependents(&index, Freshness::Fresh, "src/old.ts");

    let misses: Vec<(&str, u32, &str)> = answer
        .possible_misses
        .iter()
        .map(|miss| (miss.path.as_str(), miss.line, miss.spec.as_str()))
        .collect();
    assert_eq!(
        misses,
        [
            ("src/a.ts", 5, "../lib/old"),
            ("src/a.ts", 5, "./old"),
            ("src/b.ts", 2, "./old"),
            ("src/b.ts", 9, "./old"),
        ]
    );
}

#[test]
fn unresolved_imports_are_ordered_by_line_then_specifier() {
    let mut tree = Tree::new();
    for (spec, kind, line) in [
        ("./z", ImportKind::Static, 7),
        ("./y", ImportKind::Dynamic, 7),
        ("./x", ImportKind::Static, 3),
    ] {
        let (import, to) = missing(spec, kind, line);
        tree = tree.import("src/a.ts", import, to);
    }
    let index = tree.build();

    let answer = dependencies(&index, Freshness::Fresh, "src/a.ts");

    let unresolved: Vec<(u32, &str)> = answer
        .unresolved
        .iter()
        .map(|miss| (miss.line, miss.spec.as_str()))
        .collect();
    assert_eq!(unresolved, [(3, "./x"), (7, "./y"), (7, "./z")]);
}

#[test]
fn a_file_that_imports_nothing_says_so() {
    let index = Tree::new().imports("b.ts", "a.ts").build();

    let text = dependencies(&index, Freshness::Fresh, "a.ts").to_string();

    assert!(
        text.starts_with("a.ts imports nothing; ts found no imports in it\n"),
        "{text}"
    );
}

#[scenario(
    "queries",
    "A file that failed to parse is not one that imports nothing"
)]
#[test]
fn a_file_that_failed_to_parse_does_not_say_it_imports_nothing() {
    let index = Tree::new()
        .failed(
            "src/broken.ts",
            ParseFailure::Syntax {
                line: 3,
                message: "unexpected token".to_owned(),
            },
        )
        .build();

    let answer = dependencies(&index, Freshness::Fresh, "src/broken.ts");

    let text = answer.to_string();
    assert!(
        text.starts_with(
            "src/broken.ts could not be parsed, so its imports are unknown: \
             syntax error at line 3: unexpected token\n"
        ),
        "{text}"
    );
    assert!(!text.contains("imports nothing"), "{text}");
    let json = serde_json::to_value(&answer).unwrap();
    assert_eq!(json["status"]["status"], "failed");
    assert_eq!(json["status"]["failure"], "syntax");
}

#[scenario(
    "queries",
    "A file read past syntax errors says its imports may be missing"
)]
#[test]
fn a_file_read_past_syntax_errors_says_its_imports_may_be_missing() {
    let bare = Tree::new().recovered("src/rough.ts", 2, 7).build();
    let rich = Tree::new()
        .recovered("src/rough.ts", 1, 9)
        .imports("src/rough.ts", "src/a.ts")
        .build();

    let bare = dependencies(&bare, Freshness::Fresh, "src/rough.ts").to_string();
    let rich = dependencies(&rich, Freshness::Fresh, "src/rough.ts").to_string();

    assert!(
        bare.starts_with(
            "src/rough.ts was read past 2 syntax errors, the first at line 7, \
             so imports may be missing\n"
        ),
        "{bare}"
    );
    assert!(!bare.contains("imports nothing"), "{bare}");
    assert!(
        rich.starts_with(
            "dependencies of src/rough.ts\n\
             src/rough.ts was read past 1 syntax error, the first at line 9, \
             so imports may be missing\n\
             files (1):\n\
             src/a.ts  static import\n"
        ),
        "{rich}"
    );
}

#[test]
fn a_path_no_connector_read_is_not_in_the_index() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .unread("notes.txt")
        .build();

    for text in [
        dependents(&index, Freshness::Fresh, "notes.txt").to_string(),
        dependencies(&index, Freshness::Fresh, "notes.txt").to_string(),
        dependents(&index, Freshness::Fresh, "nowhere/at/all.ts").to_string(),
    ] {
        assert!(text.contains(" is not in the index\n"), "{text}");
    }
    let answer = dependents(&index, Freshness::Fresh, "notes.txt");
    assert!(!answer.in_index);
    assert_eq!(answer.path, "notes.txt");
}

#[test]
fn an_answer_holds_its_file_when_the_index_does() {
    let index = Tree::new()
        .imports("b.ts", "a.ts")
        .unread("notes.txt")
        .build();

    assert!(dependents(&index, Freshness::Fresh, "a.ts").held());
    assert!(dependencies(&index, Freshness::Fresh, "b.ts").held());
    assert!(!dependents(&index, Freshness::Fresh, "notes.txt").held());
    assert!(!dependencies(&index, Freshness::Fresh, "nowhere.ts").held());
}

#[test]
fn a_path_is_found_however_it_is_spelled() {
    let index = Tree::new().imports("src/b.ts", "src/a.ts").build();

    for spelling in ["src/a.ts", "./src/a.ts", "src//a.ts", "src/./a.ts"] {
        let answer = dependents(&index, Freshness::Fresh, spelling);
        assert_eq!(answer.path, "src/a.ts", "{spelling}");
        assert_eq!(answer.files.len(), 1, "{spelling}");
    }
}
