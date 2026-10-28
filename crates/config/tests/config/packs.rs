//! The rule packs: the files of `.gsttt/packs/` and the built-in packs, which the same reader reads.

use gsttt_config::builtin_packs;
use gsttt_core::Setup;

use crate::tree::{Tree, evaluate_at, fixture};

/// A pack for `rust` with one call rule, as a user writes it.
const KTOR: &str = r#"name = "ktor"
language = "rust"
detect = ["ktor"]

[[calls]]
callee = ["client.get", "client.post"]
kind = "http"
role = "calls"
key = "$arg1"
method = "$callee"
"#;

/// The names of the packs of a setup, in order.
fn names(setup: &Setup) -> Vec<&str> {
    setup.packs.iter().map(|pack| pack.name.as_str()).collect()
}

#[gsttt_scenario::scenario("config", "A pack file adds a pack")]
#[test]
fn a_pack_file_adds_a_pack() {
    let tree = Tree::empty();
    tree.write(".gsttt/packs/ktor.toml", KTOR);
    let setup = tree.evaluate().unwrap();

    let [pack] = setup.packs.as_slice() else {
        panic!("one pack expected, got {:?}", names(&setup));
    };
    assert_eq!(
        (pack.name.as_str(), pack.language.as_str()),
        ("ktor", "rust")
    );
    assert_eq!(pack.detect, ["ktor"]);
    let [call] = pack.calls.as_slice() else {
        panic!("one call rule expected, got {:?}", pack.calls);
    };
    assert_eq!(
        call.callee.iter().collect::<Vec<_>>(),
        ["client.get", "client.post"]
    );
    // Nothing but the pack differs from a repository without a config.
    let packs_only = Setup {
        packs: Vec::new(),
        ..setup
    };
    assert_eq!(packs_only, Setup::default());
}

#[test]
fn a_pack_of_a_fixture_is_added_without_a_config() {
    let setup = evaluate_at(&fixture("billing")).unwrap();
    assert_eq!(names(&setup), ["billing"]);
    assert_eq!(setup.packs[0].calls.len(), 1);
    assert_eq!(setup.packs[0].detect, ["@acme/billing"]);
}

#[gsttt_scenario::scenario("config", "A pack is named after its file")]
#[test]
fn a_pack_is_named_after_its_file() {
    let message = failure_of_pack(&KTOR.replace("name = \"ktor\"", "name = \"spring\""));
    assert!(
        message.starts_with(".gsttt/packs/ktor.toml:1:8: "),
        "the place of the name: {message}"
    );
    assert!(message.contains("`spring`"), "{message}");
    assert!(message.contains("`ktor`"), "{message}");
}

#[gsttt_scenario::scenario("config", "Only pack files are read")]
#[test]
fn only_pack_files_are_read() {
    let tree = Tree::empty();
    let pack = |name: &str| format!("name = \"{name}\"\nlanguage = \"ts\"\n");
    tree.write(".gsttt/packs/b.toml", &pack("b"));
    tree.write(".gsttt/packs/a.toml", &pack("a"));
    // Neither of these may be read: the first would fail to load, and the second is no TOML at all.
    tree.write(
        ".gsttt/packs/.draft.toml",
        "name = \"draft\"\nthis is not valid\n",
    );
    tree.write(".gsttt/packs/notes.md", "# notes\n\n[[[ not a pack\n");
    tree.write(".gsttt/packs/c.toml.bak", "not a pack either");
    let setup = tree.evaluate().unwrap();
    assert_eq!(names(&setup), ["a", "b"]);
}

#[test]
fn the_packs_of_a_repository_come_in_the_order_of_their_file_names() {
    let tree = Tree::empty();
    for name in ["zebra", "apple", "mango"] {
        tree.write(
            &format!(".gsttt/packs/{name}.toml"),
            &format!("name = \"{name}\"\nlanguage = \"python\"\n"),
        );
    }
    assert_eq!(
        names(&tree.evaluate().unwrap()),
        ["apple", "mango", "zebra"]
    );
}

#[gsttt_scenario::scenario("config", "A misspelt rule key is named with its line")]
#[test]
fn a_misspelt_rule_key_is_named_with_its_line() {
    let message = failure_of_pack(&KTOR.replace("callee =", "calee ="));
    // `calee` sits on the sixth line of the file.
    assert!(
        message.starts_with(".gsttt/packs/ktor.toml:6:"),
        "{message}"
    );
    assert!(message.contains("`calee`"), "{message}");
}

#[test]
fn a_misspelt_key_of_a_pack_is_named_with_its_line() {
    let message = failure_of_pack(&KTOR.replace("language =", "languge ="));
    assert!(
        message.starts_with(".gsttt/packs/ktor.toml:2:"),
        "{message}"
    );
    assert!(message.contains("`languge`"), "{message}");
}

#[test]
fn a_pack_without_a_language_is_refused_with_the_file() {
    let message = failure_of_pack("name = \"ktor\"\n");
    assert!(message.starts_with(".gsttt/packs/ktor.toml:"), "{message}");
    assert!(message.contains("language"), "{message}");
}

#[test]
fn a_pack_is_read_even_when_the_repository_has_no_config() {
    let tree = Tree::empty();
    tree.write(".gsttt/packs/ktor.toml", KTOR);
    assert!(!tree.root().join(".gsttt/config.toml").exists());
    assert_eq!(names(&tree.evaluate().unwrap()), ["ktor"]);
}

#[test]
fn a_built_in_pack_is_read_like_a_pack_file() {
    let packs = builtin_packs(&[("ktor", KTOR)]).unwrap();
    let from_file = {
        let tree = Tree::empty();
        tree.write(".gsttt/packs/ktor.toml", KTOR);
        tree.evaluate().unwrap().packs
    };
    assert_eq!(packs, from_file);

    // It fails the way a file does, naming the pack by its file name, which is how the connector ships it.
    let error = builtin_packs(&[("ktor", &KTOR.replace("callee =", "calee ="))]).unwrap_err();
    assert_eq!(error.file, "ktor.toml");
    assert!(error.message.starts_with("ktor.toml:6:"), "{error}");
    assert!(error.message.contains("`calee`"), "{error}");

    let renamed =
        builtin_packs(&[("ktor", &KTOR.replace("\"ktor\"\n", "\"spring\"\n"))]).unwrap_err();
    assert!(renamed.message.starts_with("ktor.toml:1:8: "), "{renamed}");
}

/// The message of `.gsttt/packs/ktor.toml` holding `text` when the repository is evaluated.
fn failure_of_pack(text: &str) -> String {
    let tree = Tree::empty();
    tree.write(".gsttt/packs/ktor.toml", text);
    tree.evaluate().unwrap_err().to_string()
}

#[test]
fn the_config_is_read_before_the_packs() {
    // The config is fine, so the failure is the pack's.
    let tree = Tree::with_config("exclude = [\"a\"]\n");
    tree.write(".gsttt/packs/ktor.toml", "name = 1\n");
    let message = tree.evaluate().unwrap_err().to_string();
    assert!(
        message.starts_with(".gsttt/packs/ktor.toml:1:"),
        "{message}"
    );

    // When both fail, the run reports the config, the first of them.
    let tree = Tree::with_config("exclude = 1\n");
    tree.write(".gsttt/packs/ktor.toml", "name = 1\n");
    let message = tree.evaluate().unwrap_err().to_string();
    assert!(message.starts_with(".gsttt/config.toml:1:"), "{message}");
}
