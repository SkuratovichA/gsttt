//! The module tree, on the `modules` fixture.

use std::fs;

use gsttt_scenario::scenario;
use insta::assert_json_snapshot;
use serde_json::json;

use crate::support::{index, index_at, unresolved};

#[scenario("rust", "Module declarations find their files")]
#[test]
fn module_declarations_find_their_files() {
    let out = index("modules");
    assert_eq!(out.to("src/lib.rs", "a"), json!({ "file": "src/a.rs" }));
    assert_eq!(out.to("src/lib.rs", "b"), json!({ "file": "src/b/mod.rs" }));
    assert_eq!(out.to("src/a.rs", "c"), json!({ "file": "src/a/c.rs" }));
    assert_json_snapshot!(out.file("src/lib.rs")["imports"]);
}

#[scenario("rust", "A path attribute names the file")]
#[test]
fn a_path_attribute_names_the_file() {
    let out = index("modules");
    assert_eq!(
        out.to("src/lib.rs", "schema"),
        json!({ "file": "src/generated/schema.rs" })
    );
}

#[scenario("rust", "A missing module file is not found")]
#[test]
fn a_missing_module_file_is_not_found() {
    let out = index("modules");
    assert_eq!(out.to("src/lib.rs", "gone"), unresolved("not-found"));
}

#[scenario("rust", "Paths stop at the deepest module")]
#[test]
fn paths_stop_at_the_deepest_module() {
    let out = index("modules");
    assert_eq!(
        out.to("src/lib.rs", "crate::a::c::Thing"),
        json!({ "file": "src/a/c.rs" })
    );
    assert_eq!(
        out.to("src/a.rs", "self::c::Thing"),
        json!({ "file": "src/a/c.rs" })
    );
    assert_eq!(
        out.to("src/a/c.rs", "super::Helper"),
        json!({ "file": "src/a.rs" })
    );
}

#[scenario("rust", "A module declared twice gives both files a module")]
#[test]
fn a_module_declared_twice_gives_both_files_a_module() {
    let out = index("cfgmods");
    // A path into the module reaches the first declaration in source order.
    assert_eq!(
        out.to("src/lib.rs", "sys::run"),
        json!({ "file": "src/unix.rs" })
    );
    // Each file resolves `crate::` and `super::` from the module it is.
    for file in ["src/unix.rs", "src/win.rs"] {
        assert_eq!(
            out.to(file, "crate::util::shared"),
            json!({ "file": "src/util.rs" }),
            "{file}"
        );
    }
    assert_eq!(
        out.to("src/win.rs", "super::util::shared"),
        json!({ "file": "src/util.rs" })
    );
    // The project says that only one of the two files is reachable by name.
    assert!(
        out.json()["projects"]
            .to_string()
            .contains("a module declared twice under different `cfg`s")
    );
}

#[scenario("rust", "A file no declaration reaches has no module")]
#[test]
fn a_file_no_declaration_reaches_has_no_module() {
    let out = index("cfgmods");
    assert_eq!(
        out.to("src/orphan.rs", "crate::util::shared"),
        unresolved("not-found")
    );
    assert_eq!(
        out.to("src/orphan.rs", "serde::Serialize"),
        json!({ "external": "serde" })
    );
}

#[scenario("rust", "A tree that the module cap cuts keeps the modules before it")]
#[test]
fn a_tree_that_the_module_cap_cuts_keeps_the_modules_before_it() {
    // Each of three files declares the next one under 22 names through `#[path]`, so that a few lines of declarations
    // grow into 22 * 22 * 22 modules at the bottom, more than the 10000 the cap lets in.
    let fan = |name: &str| -> String {
        (0..22)
            .map(|number| format!("#[path = \"{name}.rs\"] mod {name}{number};\n"))
            .collect()
    };
    let dir = tempfile::tempdir().unwrap();
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = \"wide\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".to_owned(),
        ),
        ("src/lib.rs", format!("mod user;\n{}", fan("a"))),
        ("src/a.rs", fan("b")),
        ("src/b.rs", fan("c")),
        ("src/c.rs", "pub fn item() {}\n".to_owned()),
        (
            "src/user.rs",
            "use crate::a0::b0::c0::item;\nuse crate::a21::b21::c21::item as late;\n".to_owned(),
        ),
    ];
    for (name, text) in files {
        let file = dir.path().join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    let out = index_at(dir.path().to_str().unwrap());
    // The first modules are in the tree.
    assert_eq!(
        out.to("src/user.rs", "crate::a0::b0::c0::item"),
        json!({ "file": "src/c.rs" })
    );
    // The last are cut, so the path stops at the root, the deepest module it reaches.
    assert_eq!(
        out.to("src/user.rs", "crate::a21::b21::c21::item"),
        json!({ "file": "src/lib.rs" })
    );
}
