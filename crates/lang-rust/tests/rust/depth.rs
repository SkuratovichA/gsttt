//! A file nested past the limit of the reader is one failed file, and the files beside it are read.

use std::fs;

use gsttt_scenario::scenario;

use crate::support::index_at;

/// More levels than the reader takes, in each of the shapes that nest differently in the tree.
const LEVELS: usize = 5_000;

#[scenario("rust", "A file nested past the limit is one failed file")]
#[test]
fn a_file_nested_past_the_limit_is_one_failed_file() {
    let tree = tempfile::tempdir().unwrap();
    fs::create_dir(tree.path().join("src")).unwrap();
    fs::write(
        tree.path().join("Cargo.toml"),
        "[package]\nname = \"deep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        tree.path().join("src/lib.rs"),
        "mod parens;\nmod sum;\nmod unary;\nmod open;\nuse crate::parens::P;\n",
    )
    .unwrap();
    let nested = [
        (
            "parens",
            format!(
                "pub const P: i32 = {}1{};\n",
                "(".repeat(LEVELS),
                ")".repeat(LEVELS)
            ),
        ),
        (
            "sum",
            format!("pub const S: i32 = 1{};\n", " + 1".repeat(LEVELS)),
        ),
        (
            "unary",
            format!("pub const U: i32 = {}1;\n", "-".repeat(LEVELS)),
        ),
        (
            "open",
            format!("pub const O: i32 = {}1;\n", "(".repeat(LEVELS)),
        ),
    ];
    for (name, text) in &nested {
        fs::write(tree.path().join(format!("src/{name}.rs")), text).unwrap();
    }

    let out = index_at(tree.path().to_str().unwrap());

    for (name, _) in &nested {
        let file = out.file(&format!("src/{name}.rs"));
        let status = file["status"].to_string();
        assert!(
            status.contains("nested deeper than 2000 levels"),
            "{name}: {status}"
        );
    }
    let imports = out.file("src/lib.rs")["imports"].to_string();
    assert!(imports.contains("crate::parens::P"), "{imports}");
}
