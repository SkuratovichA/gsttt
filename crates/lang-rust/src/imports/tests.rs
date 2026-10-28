//! The import reader over single files, without the engine around it.

use gsttt_core::fact::{Import, ImportKind};
use gsttt_core::text::LineIndex;
use proptest::prelude::*;
use ra_ap_edition::Edition;
use ra_ap_syntax::ast;

use super::read;

fn imports(source: &str) -> Vec<Import> {
    let parse = ast::SourceFile::parse(source, Edition::CURRENT);
    read(
        &parse.syntax_node(),
        &LineIndex::new(source),
        true,
        Edition::CURRENT,
    )
}

/// The specs of the imports, in the order they are returned.
fn specs(source: &str) -> Vec<String> {
    imports(source)
        .into_iter()
        .map(|import| import.spec)
        .collect()
}

#[test]
fn a_path_that_starts_with_an_inline_module_of_its_scope_is_made_explicit() {
    assert_eq!(
        specs("mod shapes { pub struct C; }\nuse shapes::C;\nfn f() { shapes::C; }"),
        ["self::shapes::C", "self::shapes::C"]
    );
}

#[test]
fn an_inline_module_is_found_from_inside_the_module_that_declares_it() {
    assert_eq!(
        specs("mod outer {\n    mod inner { pub struct X; }\n    use inner::X;\n}"),
        ["self::outer::inner::X"]
    );
}

#[test]
fn a_use_inside_a_function_sees_the_inline_modules_around_it() {
    assert_eq!(
        specs("mod shapes { pub struct C; }\nfn f() {\n    use shapes::C;\n}"),
        ["self::shapes::C"]
    );
}

#[test]
fn a_path_in_a_macro_sees_the_inline_modules_around_it() {
    assert_eq!(
        specs("mod shapes {}\nfn f() { m!(shapes::C); }"),
        ["self::shapes::C"]
    );
}

#[test]
fn a_name_that_no_inline_module_beside_it_declares_stays_as_written() {
    // Declared in another module, declared as a file, or not declared at all.
    assert_eq!(
        specs("mod a { mod shapes {} }\nmod files;\nuse shapes::C;\nuse files::D;\nuse other::E;"),
        ["files", "shapes::C", "files::D", "other::E"]
    );
}

/// Pieces of a file that open and close modules, bring names in and use them, and pieces that break them.
const FRAGMENTS: &[&str] = &[
    "mod shapes {",
    "mod inner {",
    "mod files;",
    "}",
    "use shapes::C;",
    "use self::shapes::{C, D as E};",
    "use self::y;",
    "use super::z;",
    "use super::super::super::z;",
    "use super::*;",
    "pub use shapes::inner::*;",
    "use crate::{a, b::c};",
    "extern crate serde as s;",
    "fn f() { shapes::g(); inner::h(); }",
    "#[derive(serde::Serialize)]",
    "#[cfg(test)]",
    "#[path = \"p.rs\"]",
    "include!(\"x.rs\");",
    "m!(shapes::C, self::inner::D);",
    "::",
    "*",
    "{",
    "(",
    ")",
    ";",
    "\n",
];

proptest! {
    #[test]
    fn reading_any_arrangement_of_fragments_is_total_and_stable(
        parts in prop::collection::vec(prop::sample::select(FRAGMENTS), 0..30),
    ) {
        let source = parts.join(" ");
        let first = imports(&source);
        let last_line = u32::try_from(source.matches('\n').count()).unwrap() + 1;
        prop_assert!(first.iter().all(|import| (1..=last_line).contains(&import.line)));
        prop_assert_eq!(first, imports(&source));
    }

    #[test]
    fn reading_arbitrary_text_never_panics(source in ".{0,200}") {
        // Reaching the end is the assertion: a panic in the reader fails the case.
        drop(imports(&source));
    }
}

#[test]
fn a_use_path_in_2015_starts_at_the_crate_root() {
    let source = "use a::b;\nuse self::c::d;\nuse super::e;\nuse crate::f;\nuse ::g::h;";
    let parse = ast::SourceFile::parse(source, Edition::Edition2015);
    let found = read(
        &parse.syntax_node(),
        &LineIndex::new(source),
        true,
        Edition::Edition2015,
    );
    let specs: Vec<&str> = found.iter().map(|import| import.spec.as_str()).collect();
    assert_eq!(
        specs,
        ["::a::b", "self::c::d", "super::e", "crate::f", "::g::h"]
    );
}

#[gsttt_scenario::scenario("rust", "A path attribute may be a raw string or sit in cfg_attr")]
#[test]
fn a_path_attribute_may_be_a_raw_string_or_sit_inside_cfg_attr() {
    let names = |source: &str| -> Vec<Vec<String>> {
        imports(source)
            .into_iter()
            .map(|import| import.names)
            .collect()
    };
    assert_eq!(names("#[path = r\"raw.rs\"]\nmod a;"), [["raw.rs"]]);
    assert_eq!(names("#[path = r#\"hash.rs\"#]\nmod a;"), [["hash.rs"]]);
    assert_eq!(
        names("#[cfg_attr(unix, path = \"unix.rs\")]\nmod a;"),
        [["unix.rs"]]
    );
}

#[gsttt_scenario::scenario("rust", "A derive inside cfg_attr names its crate")]
#[test]
fn a_derive_inside_cfg_attr_names_its_crate() {
    assert_eq!(
        specs("#[cfg_attr(feature = \"x\", derive(serde::Serialize, Debug))]\nstruct S;"),
        ["serde::Serialize"]
    );
}

#[gsttt_scenario::scenario("rust", "A module declared inside cfg_if is read")]
#[test]
fn modules_declared_inside_cfg_if_are_read() {
    let found = imports(
        "cfg_if::cfg_if! {\n    if #[cfg(unix)] {\n        mod unix;\n    } else {\n        mod win;\n    }\n}",
    );
    let modules: Vec<&str> = found
        .iter()
        .filter(|import| import.kind == ImportKind::Module)
        .map(|import| import.spec.as_str())
        .collect();
    assert_eq!(modules, ["unix", "win"]);
}
