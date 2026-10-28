//! The config file read into a setup: its keys, its tables, its errors, its option tables and its fingerprint.

use gsttt_config::fingerprint;
use gsttt_core::Setup;
use proptest::prelude::*;
use serde_json::{Value, json};

use crate::tree::{Tree, evaluate_at, failure, fixture};

/// The modules of a setup with their options, in the order the setup lists them.
fn modules(setup: &Setup) -> Vec<(&str, &Value)> {
    setup
        .modules
        .iter()
        .map(|module| (module.module.as_str(), &module.options))
        .collect()
}

#[gsttt_scenario::scenario("config", "The keys fill the setup")]
#[test]
fn the_keys_fill_the_setup() {
    let setup = evaluate_at(&fixture("full")).unwrap();
    insta::assert_json_snapshot!(setup);
}

#[test]
fn the_keys_that_are_left_out_keep_their_defaults() {
    let setup = Tree::with_config("detect = false\nmax_file_bytes = 1_000_000\n")
        .evaluate()
        .unwrap();
    assert_eq!(
        setup,
        Setup {
            detect: false,
            max_file_bytes: 1_000_000,
            ..Setup::default()
        }
    );
}

#[gsttt_scenario::scenario("config", "An empty table turns a module on")]
#[test]
fn an_empty_table_turns_a_module_on() {
    let setup = Tree::with_config("detect = false\n\n[python]\n")
        .evaluate()
        .unwrap();
    assert_eq!(modules(&setup), [("python", &json!(null))]);
    assert!(setup.runs("python", || false));
    assert!(setup.options("python").is_null());
    assert!(!setup.runs("ts", || true));
    assert!(!setup.runs("rust", || true));
}

#[gsttt_scenario::scenario("config", "The order of the tables is not the order of the modules")]
#[test]
fn the_tables_enter_the_setup_in_the_order_of_the_registry() {
    let setup = Tree::with_config("[rust]\n\n[ts]\ntsconfig = \"a.json\"\n")
        .evaluate()
        .unwrap();
    let names: Vec<&str> = modules(&setup).iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["ts", "rust"]);
}

#[test]
fn externals_enter_the_setup_in_the_order_of_the_file() {
    let declare =
        |name: &str| format!("[[external]]\nname = \"{name}\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\n");
    let setup = Tree::with_config(&format!("{}\n{}", declare("zeta"), declare("alpha")))
        .evaluate()
        .unwrap();
    let names: Vec<&str> = setup
        .externals
        .iter()
        .map(|external| external.name.as_str())
        .collect();
    assert_eq!(names, ["zeta", "alpha"]);
}

#[gsttt_scenario::scenario("config", "Disable names a known module")]
#[test]
fn disable_names_a_known_module() {
    let message = failure("detect = true\ndisable = [\"openapi\", \"tss\"]\n");
    assert!(message.starts_with(".gsttt/config.toml:2:"), "{message}");
    assert!(message.contains("unknown module `tss`"), "{message}");
    assert!(message.contains("ts, python, rust, openapi"), "{message}");
}

#[gsttt_scenario::scenario("config", "No config means defaults")]
#[test]
fn a_repository_without_a_config_gets_the_defaults() {
    let setup = Tree::empty().evaluate().unwrap();
    assert_eq!(setup, Setup::default());
    assert!(setup.detect);
    assert!(setup.modules.is_empty() && setup.packs.is_empty() && setup.externals.is_empty());
    assert!(setup.disable.is_empty() && setup.disable_packs.is_empty());
    assert_eq!(setup.max_file_bytes, 2 * 1024 * 1024);
}

#[gsttt_scenario::scenario("config", "An error names its line")]
#[test]
fn an_error_starts_with_the_file_and_the_line() {
    let message = failure("exclude = []\nhttp_prefixes = []\nmax_file_bytes = \"big\"\n");
    assert!(message.starts_with(".gsttt/config.toml:3:"), "{message}");
    assert!(message.contains("`max_file_bytes`"), "{message}");
}

#[gsttt_scenario::scenario("config", "A syntax error names its line and column")]
#[test]
fn a_syntax_error_names_its_line_and_its_column_in_characters() {
    let message = failure("detect = true\nexclude = [\"ä\" \"b\"]\n");
    // The second string starts at the 16th character of the line and at its 17th byte.
    assert!(
        message.starts_with(".gsttt/config.toml:2:16: "),
        "{message}"
    );
}

#[gsttt_scenario::scenario("config", "A long error is cut")]
#[test]
fn a_long_error_is_cut_to_a_kilobyte() {
    let message = failure(&format!("\"{}\" = 1\n", "k".repeat(100_000)));
    assert_eq!(message.len(), 1024);
    assert!(message.starts_with(".gsttt/config.toml:1:1: "), "{message}");
    assert!(message.ends_with('…'), "{message}");
}

#[gsttt_scenario::scenario("config", "A misspelt module is named with the known ones")]
#[test]
fn a_misspelt_module_is_named_with_the_known_ones() {
    let message = failure("detect = true\n\n[tss]\n");
    assert!(message.starts_with(".gsttt/config.toml:3:"), "{message}");
    assert!(message.contains("`tss`"), "{message}");
    assert!(message.contains("ts, python, rust, openapi"), "{message}");
}

#[gsttt_scenario::scenario("config", "A misspelt key is named")]
#[test]
fn a_misspelt_key_is_named_with_the_known_keys() {
    let message = failure("exlude = [\"a\"]\n");
    assert!(message.starts_with(".gsttt/config.toml:1:"), "{message}");
    assert!(message.contains("`exlude`"), "{message}");
    for key in [
        "exclude",
        "http_prefixes",
        "max_file_bytes",
        "detect",
        "disable",
        "disable_packs",
        "external",
    ] {
        assert!(message.contains(key), "{key} is missing in {message}");
    }
}

#[test]
fn the_first_unknown_key_of_the_file_is_the_one_named() {
    // Not the first by name: `aaa` sorts before `zzz`.
    let message = failure("zzz = 1\naaa = 2\n");
    assert!(message.starts_with(".gsttt/config.toml:1:"), "{message}");
    assert!(
        message.contains("`zzz`") && !message.contains("`aaa`"),
        "{message}"
    );
}

#[gsttt_scenario::scenario("config", "A misspelt key of an external is named")]
#[test]
fn a_misspelt_key_of_an_external_is_named() {
    let message = failure("[[external]]\nname = \"k\"\nfiles = [\"*.k\"]\ncomand = [\"x\"]\n");
    assert!(message.starts_with(".gsttt/config.toml:4:"), "{message}");
    assert!(message.contains("`comand`"), "{message}");
}

#[test]
fn a_module_takes_a_table() {
    let message = failure("ts = 1\n");
    assert!(message.starts_with(".gsttt/config.toml:1:"), "{message}");
    assert!(
        message.contains("`ts` must be a table of options") && message.contains("integer"),
        "{message}"
    );
}

#[test]
fn a_value_of_the_wrong_type_is_named_by_its_path() {
    let message = failure("[[external]]\nname = \"k\"\ncmd = \"x\"\nfiles = [\"*.k\"]\n");
    assert!(message.starts_with(".gsttt/config.toml:3:"), "{message}");
    assert!(message.contains("`external[0].cmd`"), "{message}");
}

#[gsttt_scenario::scenario("config", "Tables become lists and maps")]
#[test]
fn tables_become_lists_and_maps() {
    let setup = Tree::with_config(
        r#"
[ts]
paths = { "@app/*" = ["src/*"] }
conditions = ["import", "types"]
extra = []
"#,
    )
    .evaluate()
    .unwrap();
    assert_eq!(
        setup.options("ts"),
        &json!({
            "conditions": ["import", "types"],
            "extra": [],
            "paths": { "@app/*": ["src/*"] },
        })
    );
    // The keys of a map are sorted, which the text shows and the comparison of values does not.
    assert_eq!(
        serde_json::to_string(setup.options("ts")).unwrap(),
        r#"{"conditions":["import","types"],"extra":[],"paths":{"@app/*":["src/*"]}}"#
    );

    // Every level is sorted and not only the top one: the tables nested in this one are written out of order.
    let nested = Tree::with_config("[ts]\nouter = { b = { z = 1, y = 2 }, a = 3 }\n")
        .evaluate()
        .unwrap();
    assert_eq!(
        serde_json::to_string(nested.options("ts")).unwrap(),
        r#"{"outer":{"a":3,"b":{"y":2,"z":1}}}"#
    );
}

#[gsttt_scenario::scenario("config", "A value with no JSON form is refused")]
#[test]
fn a_value_with_no_json_form_is_refused_with_its_key_and_line() {
    for (line, key) in [
        ("since = 1979-05-27", "ts.since"),
        ("limit = nan", "ts.limit"),
    ] {
        let message = failure(&format!("[ts]\n{line}\n"));
        assert!(message.starts_with(".gsttt/config.toml:2:"), "{message}");
        assert!(message.contains(&format!("`{key}`")), "{message}");
        assert!(message.contains("no JSON form"), "{message}");
    }
}

#[test]
fn a_path_reads_the_same_in_both_kinds_of_error() {
    // The walk that refuses a value with no JSON form prints its path with `dotted`, and a value of the wrong type is named by `with_path` from the path serde keeps: two printers of one syntax, pinned here against each other and against the place as the config writes it.
    let places = [
        ("exclude[0]", "exclude = [VALUE]\n"),
        (
            "external[0].cmd[0]",
            "[[external]]\nname = \"k\"\ncmd = [VALUE]\nfiles = [\"*.x\"]\n",
        ),
        (
            "external[0].provides[0].via[0]",
            "[[external]]\nname = \"k\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\n\n[[external.provides]]\nkind = \"http\"\nrole = \"serves\"\nvia = [VALUE]\nblind_spots = []\n",
        ),
    ];
    let quoted = |message: &str| message.split('`').nth(1).map(str::to_owned);
    for (place, config) in places {
        let date = failure(&config.replace("VALUE", "1979-05-27"));
        assert!(date.contains("no JSON form"), "{date}");
        assert_eq!(quoted(&date).as_deref(), Some(place), "{date}");
        let integer = failure(&config.replace("VALUE", "1"));
        assert!(integer.contains("invalid type"), "{integer}");
        assert_eq!(quoted(&integer).as_deref(), Some(place), "{integer}");
    }
}

#[gsttt_scenario::scenario("config", "Comments do not change the fingerprint")]
#[test]
fn comments_do_not_change_the_fingerprint() {
    let print = |config: &str| fingerprint(&Tree::with_config(config).evaluate().unwrap()).unwrap();
    let plain = "http_prefixes = [\"/api\"]\n";
    let commented = format!("# the gateway prefix\n{plain}# the end\n");
    assert_eq!(print(plain), print(&commented));
    assert_ne!(print(plain), print(&plain.replace("/api", "/v2")));

    // A value in the options of a module is part of the setup, and so is one in the options of an external.
    let module = |tsconfig: &str| format!("[ts]\ntsconfig = \"{tsconfig}\"\n");
    assert_ne!(print(&module("a.json")), print(&module("b.json")));
    let external = |strict: bool| {
        format!(
            "[[external]]\nname = \"k\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\noptions = {{ strict = {strict} }}\n"
        )
    };
    assert_ne!(print(&external(true)), print(&external(false)));
}

#[gsttt_scenario::scenario("config", "Moving a module's table does not change the fingerprint")]
#[test]
fn moving_a_modules_table_does_not_change_the_fingerprint() {
    let ts = "[ts]\ntsconfig = \"a.json\"\nconditions = [\"import\"]\n";
    let reordered = "[ts]\nconditions = [\"import\"]\ntsconfig = \"a.json\"\n";
    let python = "[python]\n";
    let setup = |config: String| Tree::with_config(&config).evaluate().unwrap();

    let first = setup(format!("exclude = [\"a\"]\n\n{ts}\n{python}"));
    let last = setup(format!("exclude = [\"a\"]\n\n{python}\n{ts}"));
    let keys_moved = setup(format!("exclude = [\"a\"]\n\n{reordered}\n{python}"));
    assert_eq!(first, last);
    assert_eq!(first, keys_moved);
    assert_eq!(fingerprint(&first).unwrap(), fingerprint(&last).unwrap());
    assert_eq!(
        fingerprint(&first).unwrap(),
        fingerprint(&keys_moved).unwrap()
    );
}

#[test]
fn an_external_is_checked_when_it_is_declared() {
    let declare = |fields: &str| failure(&format!("[[external]]\n{fields}\n"));
    let named = declare("name = \"bad name\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]");
    assert!(
        named.starts_with(".gsttt/config.toml:1:")
            && named.contains("the name `bad name` must be 1 to 64 letters, digits"),
        "{named}"
    );
    let long = declare(&format!(
        "name = \"{}\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]",
        "x".repeat(65)
    ));
    assert!(long.contains("must be 1 to 64 letters, digits"), "{long}");
    let module = declare("name = \"ts\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]");
    assert!(module.contains("already a module of gsttt"), "{module}");
    let command = declare("name = \"k\"\ncmd = []\nfiles = [\"*.x\"]");
    assert!(command.contains("needs a `cmd`"), "{command}");
    let files = declare("name = \"k\"\ncmd = [\"x\"]\nfiles = []");
    assert!(files.contains("needs `files`"), "{files}");
    let glob = declare("name = \"k\"\ncmd = [\"x\"]\nfiles = [\"[\"]");
    assert!(glob.contains("holds `[`"), "{glob}");
    let env = declare("name = \"k\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\nenv = [\"A=B\"]");
    assert!(env.contains("`env` of `k`") && env.contains("A=B"), "{env}");
    let missing = declare("name = \"k\"");
    assert!(missing.contains("missing field"), "{missing}");

    let entry = "[[external]]\nname = \"k\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\n";
    let twice = failure(&format!("{entry}\n{entry}"));
    assert!(
        twice.starts_with(".gsttt/config.toml:6:") && twice.contains("`k` is declared twice"),
        "{twice}"
    );
}

#[gsttt_scenario::scenario("config", "Options must be named")]
#[test]
fn the_options_of_an_external_must_be_named() {
    for (options, found) in [
        ("[1, 2]", "a list"),
        ("[]", "a list"),
        ("3", "a number"),
        ("\"x\"", "a string"),
        ("true", "a boolean"),
    ] {
        let message = failure(&format!(
            "[[external]]\nname = \"k\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\noptions = {options}\n"
        ));
        assert!(message.starts_with(".gsttt/config.toml:1:"), "{message}");
        assert!(
            message.contains(&format!(
                "`options` of `k` must be a table of named options, not {found}"
            )),
            "{options}: {message}"
        );
    }

    // Left out or empty, they are the same: no options.
    let options = |config: &str| {
        let setup = Tree::with_config(config).evaluate().unwrap();
        setup.externals[0].options.clone()
    };
    let entry = "[[external]]\nname = \"k\"\ncmd = [\"x\"]\nfiles = [\"*.x\"]\n";
    assert_eq!(options(entry), json!({}));
    assert_eq!(options(&format!("{entry}options = {{}}\n")), json!({}));
    // A table of named values becomes an object with its keys sorted.
    let named = options(&format!("{entry}options = {{ b = 1, a = [2] }}\n"));
    assert_eq!(named, json!({ "a": [2], "b": 1 }));
    assert_eq!(serde_json::to_string(&named).unwrap(), r#"{"a":[2],"b":1}"#);
    // At every level, not only at the top.
    let nested = options(&format!(
        "{entry}options = {{ outer = {{ b = {{ z = 1, y = 2 }}, a = 3 }}, c = 1 }}\n"
    ));
    assert_eq!(
        serde_json::to_string(&nested).unwrap(),
        r#"{"c":1,"outer":{"a":3,"b":{"y":2,"z":1}}}"#
    );
}

/// Pieces of a config that make the parser, the sorting of the top level and the checks of an external go as far as they can when they are put together without care.
const PIECES: &[&str] = &[
    "[ts]",
    "[tss]",
    "[[external]]",
    "[[ts.a]]",
    "name",
    "cmd",
    "files",
    "options",
    "exclude",
    "disable",
    "ts",
    "=",
    " = ",
    "\"",
    "'",
    "[",
    "]",
    "{",
    "}",
    ",",
    ".",
    "\n",
    " ",
    "1",
    "-1",
    "nan",
    "inf",
    "1979-05-27",
    "true",
    "\"x\"",
    "#",
    "ä",
    "\\",
    "\"\"\"",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// However a config is put together, the run ends in a setup or in one short error that names the file and the place, and never in a panic.
    #[test]
    fn any_config_ends_in_a_setup_or_in_one_short_error(
        pieces in prop::collection::vec(prop::sample::select(PIECES), 0..40)
    ) {
        let config: String = pieces.concat();
        match Tree::with_config(&config).evaluate() {
            Ok(setup) => prop_assert!(fingerprint(&setup).is_ok()),
            Err(error) => {
                let message = error.to_string();
                prop_assert!(message.len() <= 1024, "{} bytes", message.len());
                prop_assert!(message.starts_with(".gsttt/config.toml:"), "{message}");
            }
        }
    }
}
