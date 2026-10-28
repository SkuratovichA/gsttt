//! Editor support: the first line of the starter config, and the starter read as a config.

use std::path::{Path, PathBuf};

use gsttt_config::{CONFIG_PATH, CONFIG_SCHEMA_PATH, PACK_SCHEMA_PATH, PACKS_PATH, starter};
use gsttt_core::Setup;
use gsttt_core::setup::ModuleSetup;
use serde_json::Value;

use crate::tree::Tree;

/// The path a `#:schema` directive names, as the folder of the file that holds it resolves it.
fn schema_named_by(first_line: &str, folder: &Path) -> PathBuf {
    let directive = first_line.strip_prefix("#:schema ").unwrap();
    // `.` and `..` are resolved by hand, which is what the editor does and what `canonicalize` does not do for a file that is not there.
    let mut resolved = folder.to_path_buf();
    for part in directive.split('/') {
        match part {
            "." => {}
            ".." => assert!(resolved.pop(), "{directive} leaves the repository"),
            name => resolved.push(name),
        }
    }
    resolved
}

#[gsttt_scenario::scenario("config", "The editor settings are relative to the config folder")]
#[test]
fn the_editor_settings_are_relative_to_the_config_folder() {
    for detected in [&[][..], &["ts"][..]] {
        let text = starter(detected);
        let first = text.lines().next().unwrap();
        assert_eq!(first, "#:schema ./schema/config.json");
        // The editor resolves the path against the folder of the file, and that is where the schema is written.
        let folder = Path::new(CONFIG_PATH).parent().unwrap();
        assert_eq!(
            schema_named_by(first, folder),
            Path::new(CONFIG_SCHEMA_PATH)
        );
    }
}

#[test]
fn the_starter_tells_where_a_pack_finds_its_schema() {
    let text = starter(&[]);
    let directive = "#:schema ../schema/pack.json";
    assert!(text.contains(&format!("`{directive}`")), "{text}");
    // The packs sit one folder deeper than the config, so their directive goes up once.
    assert_eq!(
        schema_named_by(directive, Path::new(PACKS_PATH)),
        Path::new(PACK_SCHEMA_PATH)
    );
}

#[gsttt_scenario::scenario("config", "The starter is a valid config")]
#[test]
fn the_starter_is_a_valid_config() {
    let text = starter(&["ts", "python"]);
    // As it is, it changes nothing: a detected module runs with its defaults without a line.
    assert_eq!(
        Tree::with_config(&text).evaluate().unwrap(),
        Setup::default()
    );

    // With the `# ` taken off its two tables, it turns the two modules on, with no options.
    let uncommented: String = text
        .lines()
        .map(|line| match line.strip_prefix("# [") {
            Some(rest) => format!("[{rest}\n"),
            None => format!("{line}\n"),
        })
        .collect();
    assert!(uncommented.contains("\n[ts]\n"), "{uncommented}");
    assert!(uncommented.contains("\n[python]\n"), "{uncommented}");
    let on = |module: &str| ModuleSetup {
        module: module.to_owned(),
        options: Value::Null,
    };
    assert_eq!(
        Tree::with_config(&uncommented).evaluate().unwrap(),
        Setup {
            modules: vec![on("ts"), on("python")],
            ..Setup::default()
        }
    );
}

#[test]
fn the_starter_lists_the_detected_modules_once_and_in_order() {
    let text = starter(&["ts", "rust", "ts"]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.iter().filter(|line| **line == "# [ts]").count(), 1);
    assert!(lines.contains(&"# [rust]"));
    assert!(!text.contains("python"));
    let position = |table: &str| lines.iter().position(|line| *line == table).unwrap();
    assert!(position("# [rust]") < position("# [ts]"), "{text}");
    assert_eq!(
        Tree::with_config(&text).evaluate().unwrap(),
        Setup::default()
    );
}

#[test]
fn the_starter_says_when_nothing_was_detected() {
    let text = starter(&[]);
    assert!(text.contains("No module was detected"), "{text}");
    assert_eq!(
        Tree::with_config(&text).evaluate().unwrap(),
        Setup::default()
    );
}
