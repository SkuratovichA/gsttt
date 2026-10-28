//! Editor support: the `LuaLS` annotations, the language server settings and the starter config.

use std::path::{Path, PathBuf};

use gsttt_config::{ANNOTATIONS_PATH, LUARC_PATH, annotations, luarc, starter};
use gsttt_core::Setup;
use gsttt_core::connector::{DynConnector, Erased};
use mlua::{Lua, Table, Value};

use crate::fakes::{Fake, NoOptions, Registry};
use crate::tree::Tree;

fn standard_annotations() -> String {
    let registry = Registry::standard();
    annotations(&registry.connectors())
}

#[gsttt_scenario::scenario("config", "Options become annotated classes")]
#[test]
fn options_become_annotated_classes() {
    let text = standard_annotations();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.first(), Some(&"---@meta gsttt"));
    for expected in [
        "---@class gsttt.TsOptions",
        "---@field tsconfig? string The tsconfig the project reads, relative to the repository root.",
        "---@field conditions? string[] Export conditions to follow, in order.",
        "---@param options? gsttt.TsOptions",
        "function gsttt.ts(options) end",
    ] {
        assert!(lines.contains(&expected), "missing {expected:?} in\n{text}");
    }
    assert!(
        text.contains("---@param options? gsttt.TsOptions\nfunction gsttt.ts(options) end"),
        "the function must take the class right where it is declared"
    );
}

#[test]
fn the_whole_annotation_file_is_pinned() {
    insta::assert_snapshot!(standard_annotations());
}

#[test]
fn the_annotation_file_is_lua_that_declares_every_function() {
    let text = standard_annotations();
    let lua = Lua::new();
    let module: Table = lua.load(text.as_str()).eval().unwrap();
    for name in [
        "ts",
        "python",
        "rust",
        "openapi",
        "setup",
        "disable",
        "pack",
        "disable_pack",
        "external",
    ] {
        let function = module.get::<Value>(name).unwrap();
        assert!(
            matches!(function, Value::Function(_)),
            "gsttt.{name} is not declared"
        );
    }
}

#[test]
fn the_output_does_not_depend_on_the_order_of_the_registry() {
    let forward = Registry::standard();
    let mut backward = forward.connectors();
    backward.reverse();
    assert_eq!(annotations(&forward.connectors()), annotations(&backward));
}

#[test]
fn a_module_that_is_not_a_lua_identifier_gets_no_function() {
    let odd: Box<dyn DynConnector> = Box::new(Erased(Fake::<NoOptions>::new("lang-ts")));
    let text = annotations(&[odd.as_ref()]);
    assert!(!text.contains("lang-ts"), "{text}");
    let lua = Lua::new();
    lua.load(text.as_str()).exec().unwrap();
}

#[gsttt_scenario::scenario("config", "The editor settings are relative to the config folder")]
#[test]
fn the_editor_settings_are_relative_to_the_config_folder() {
    // The language server reads the settings from the root of the folder it is opened in, which is the folder they are written to.
    assert_eq!(LUARC_PATH, ".gsttt/.luarc.json");
    let workspace = Path::new(LUARC_PATH).parent().unwrap();
    let settings: serde_json::Value = serde_json::from_str(&luarc()).unwrap();
    assert_eq!(settings["runtime.version"], "Lua 5.4");

    // The library is the folder of the annotations ...
    let library: Vec<String> =
        serde_json::from_value(settings["workspace.library"].clone()).unwrap();
    let folders: Vec<PathBuf> = library
        .iter()
        .map(|folder| workspace.join(folder))
        .collect();
    let annotations_folder = Path::new(ANNOTATIONS_PATH).parent().unwrap();
    assert_eq!(folders, vec![annotations_folder.to_path_buf()]);

    // ... and the patterns find the files `require` reads, which it does read: the config loads both modules.
    let tree = Tree::with_init("require('shared')\nrequire('deep.folder')\n");
    tree.write(".gsttt/lua/shared.lua", "return {}");
    tree.write(".gsttt/lua/deep/folder/init.lua", "return {}");
    tree.evaluate().unwrap();
    let patterns: Vec<String> = serde_json::from_value(settings["runtime.path"].clone()).unwrap();
    let found = |module: &str| -> Vec<PathBuf> {
        let relative = module.replace('.', "/");
        patterns
            .iter()
            .map(|pattern| workspace.join(pattern.replace('?', &relative)))
            .filter(|candidate| tree.root().join(candidate).is_file())
            .collect()
    };
    assert_eq!(
        found("shared"),
        vec![PathBuf::from(".gsttt/lua/shared.lua")]
    );
    assert_eq!(
        found("deep.folder"),
        vec![PathBuf::from(".gsttt/lua/deep/folder/init.lua")]
    );

    // The settings cannot say where to open the editor, so the first line of the starter does.
    let note = format!("open the {} folder as the workspace", workspace.display());
    for detected in [&[][..], &["ts"][..]] {
        let text = starter(detected);
        let first = text.lines().next().unwrap();
        assert!(first.starts_with("--") && first.contains(&note), "{first}");
    }
}

#[test]
fn the_starter_lists_the_detected_modules_as_comments_and_changes_nothing() {
    let text = starter(&["ts", "rust", "ts"]);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.contains(&"local g = require(\"gsttt\")"));
    assert_eq!(
        lines.iter().filter(|line| **line == "-- g.ts {}").count(),
        1
    );
    assert!(lines.contains(&"-- g.rust {}"));
    assert!(!text.contains("python"));
    assert_eq!(Tree::with_init(&text).evaluate().unwrap(), Setup::default());
}

#[test]
fn the_starter_says_when_nothing_was_detected() {
    let text = starter(&[]);
    assert!(text.contains("No module was detected"), "{text}");
    assert_eq!(Tree::with_init(&text).evaluate().unwrap(), Setup::default());
}
