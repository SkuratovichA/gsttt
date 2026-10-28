//! `gsttt config check` and `gsttt config show`: what the config says, as the two commands tell it.

use gsttt_scenario::scenario;

use super::support::{FAKE_CONFIG, Sandbox, json, write};

/// Where the config of a repository is.
const CONFIG: &str = ".gsttt/config.toml";

/// The coverage problem of the external `fake` of [`Sandbox::fake_external`] while it is not trusted.
const NOT_TRUSTED: &str = "the external connector `fake` (/bin/sh fake.sh) is not trusted in this repository; run `gsttt config trust` in a terminal to allow it";

#[scenario("cli", "config check names the file and line of an error")]
#[test]
fn config_check_names_the_file_and_line_of_an_error() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    // The table header on line 3 is never closed.
    write(&root, CONFIG, "detect = true\n\n[ts\n");

    let checked = sandbox.run_in(&root, &["config", "check"]);

    assert_eq!(checked.code, Some(1), "{checked:?}");
    assert!(
        checked.stderr.contains(".gsttt/config.toml:3:"),
        "{checked:?}"
    );
    assert_eq!(checked.stdout, "", "{checked:?}");
}

#[scenario("cli", "config check rejects an option a module does not declare")]
#[test]
fn config_check_rejects_an_option_a_module_does_not_declare() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(&root, CONFIG, "[ts]\nno_such_option = true\n");

    let checked = sandbox.run_in(&root, &["config", "check"]);

    assert_eq!(checked.code, Some(1), "{checked:?}");
    assert!(checked.stderr.contains("`ts`"), "{checked:?}");
    assert!(checked.stderr.contains("no_such_option"), "{checked:?}");
    assert_eq!(checked.stdout, "", "{checked:?}");
}

#[scenario("cli", "config show marks an untrusted external")]
#[test]
fn config_show_marks_an_untrusted_external() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    let shown = sandbox.run_in(&fake.root, &["config", "show"]);

    assert_eq!(shown.code, Some(0), "{shown:?}");
    assert_eq!(shown.stderr, "", "{shown:?}");
    assert!(
        shown
            .stdout
            .lines()
            .any(|line| line == "  fake: not trusted (/bin/sh fake.sh)"),
        "{shown:?}"
    );
}

/// The built-in modules in the order the spec gives for the registry, written out here so that the test does not read the order from the code it checks. The unit test of the registry holds the same list as `ORDER`, because a test of the binary's library cannot share a constant with this one.
const REGISTRY_ORDER: [&str; 9] = [
    "ts", "python", "rust", "openapi", "graphql", "proto", "prisma", "sql", "env",
];

#[scenario("cli", "config show lists the modules in registry order")]
#[test]
fn config_show_lists_the_modules_in_registry_order() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    assert!(!root.join(CONFIG).exists(), "the repository has a config");

    let shown = json(&sandbox.run_in(&root, &["--json", "config", "show"]));

    let modules = shown["modules"].as_array().unwrap();
    let names: Vec<_> = modules
        .iter()
        .map(|module| module["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, REGISTRY_ORDER, "{shown}");
    // `options` is what the config gives a module, so every module has the key and no config makes it `null`.
    for module in modules {
        assert_eq!(
            module.get("options"),
            Some(&serde_json::Value::Null),
            "{module}"
        );
    }

    let text = sandbox.run_in(&root, &["config", "show"]);
    assert_eq!(text.code, Some(0), "{text:?}");
    let lines: Vec<_> = text
        .stdout
        .lines()
        .skip_while(|line| *line != "modules:")
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .collect();
    let listed: Vec<_> = lines
        .iter()
        .map(|line| line.trim_start().split(':').next().unwrap())
        .collect();
    assert_eq!(listed, REGISTRY_ORDER, "{text:?}");
    // A line shows `, options {…}` only for a module the config gives some.
    assert!(
        lines.iter().all(|line| !line.contains("options")),
        "{text:?}"
    );
}

/// An option that a module does not take stops every command that opens the repository, not only `config check`,
/// and the error names the module and the option.
#[test]
fn a_bad_option_stops_every_command_that_opens_the_repository() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(&root, CONFIG, "[ts]\nno_such_option = true\n");

    for args in [
        &["index"][..],
        &["status"][..],
        &["coverage"][..],
        &["impact", "app/a.ts"][..],
        &["dependents", "app/a.ts"][..],
        &["config", "show"][..],
    ] {
        let ran = sandbox.run_in(&root, args);
        assert_eq!(ran.code, Some(1), "{args:?}: {ran:?}");
        assert_eq!(ran.stdout, "", "{args:?}: {ran:?}");
        assert!(
            ran.stderr.contains("`ts`") && ran.stderr.contains("no_such_option"),
            "{args:?}: {ran:?}"
        );
    }
}

/// The value of an option that the module cannot read is an error too, and names the option.
#[test]
fn config_check_rejects_an_option_whose_value_the_module_cannot_read() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(&root, CONFIG, "[ts]\ntsconfig = 3\n");

    let checked = sandbox.run_in(&root, &["config", "check"]);

    assert_eq!(checked.code, Some(1), "{checked:?}");
    assert!(
        checked.stderr.contains("`ts`") && checked.stderr.contains("tsconfig"),
        "{checked:?}"
    );
}

#[test]
fn config_check_says_the_config_is_valid_and_names_each_untrusted_external() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    let checked = sandbox.run_in(&fake.root, &["config", "check"]);

    assert_eq!(checked.code, Some(0), "{checked:?}");
    assert_eq!(checked.stderr, "", "{checked:?}");
    assert_eq!(
        checked.stdout,
        format!("The config is valid.\n{NOT_TRUSTED}\n")
    );
}

#[test]
fn config_check_accepts_a_repository_with_no_config_and_ignores_json() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();

    for args in [&["config", "check"][..], &["--json", "config", "check"][..]] {
        let checked = sandbox.run_in(&root, args);
        assert_eq!(checked.code, Some(0), "{args:?}: {checked:?}");
        assert_eq!(checked.stdout, "The config is valid.\n", "{args:?}");
        assert_eq!(checked.stderr, "", "{args:?}");
    }
}

#[test]
fn config_show_lists_the_modules_with_their_state_and_options() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(&root, CONFIG, "[ts]\ntsconfig = \"tsconfig.base.json\"\n");

    let shown = sandbox.run_in(&root, &["config", "show"]);
    assert_eq!(shown.code, Some(0), "{shown:?}");
    assert!(
        shown.stdout.lines().any(|line| line
            == "  ts: on (in the config), options {\"tsconfig\":\"tsconfig.base.json\"}"),
        "{shown:?}"
    );

    // Without a table of its own the module is on because the tree holds TypeScript, and has no options to show.
    write(&root, CONFIG, "");
    let detected = sandbox.run_in(&root, &["config", "show"]);
    assert!(
        detected
            .stdout
            .lines()
            .any(|line| line == "  ts: on (detected)"),
        "{detected:?}"
    );

    write(&root, CONFIG, "disable = [\"ts\"]\n");
    let off = sandbox.run_in(&root, &["config", "show"]);
    assert!(
        off.stdout
            .lines()
            .any(|line| line == "  ts: off (disabled)"),
        "{off:?}"
    );

    write(&root, CONFIG, "detect = false\n");
    let blind = sandbox.run_in(&root, &["config", "show"]);
    assert!(
        blind
            .stdout
            .lines()
            .any(|line| line == "  ts: off (detection is off)"),
        "{blind:?}"
    );
}

#[test]
fn config_show_prints_one_document_under_json() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    let config = format!("[ts]\ntsconfig = \"tsconfig.base.json\"\n\n{FAKE_CONFIG}");
    write(&fake.root, CONFIG, &config);

    let shown = json(&sandbox.run_in(&fake.root, &["--json", "config", "show"]));

    let modules = shown["modules"].as_array().unwrap();
    let ts = modules
        .iter()
        .find(|module| module["name"] == "ts")
        .unwrap();
    assert_eq!(ts["on"], true, "{shown}");
    assert_eq!(ts["why"], "configured", "{shown}");
    assert_eq!(
        ts["options"],
        serde_json::json!({"tsconfig": "tsconfig.base.json"})
    );
    // An external the user does not trust is no module of the run.
    assert!(
        modules.iter().all(|module| module["name"] != "fake"),
        "{shown}"
    );
    assert_eq!(
        shown["externals"],
        serde_json::json!([{
            "name": "fake",
            "trusted": false,
            "cmd": ["/bin/sh", "fake.sh"],
            "files": ["**/*.fake"],
        }])
    );
    // The packs of a module that is on are listed, and these are the built-in ones.
    let packs = shown["packs"].as_array().unwrap();
    assert!(
        packs
            .iter()
            .any(|pack| pack["language"] == "ts" && pack["origin"] == "built-in"),
        "{shown}"
    );
    assert!(
        packs.iter().all(|pack| pack["origin"] == "built-in"),
        "{shown}"
    );
    assert_eq!(shown["detect"], true);
    assert_eq!(shown["exclude"], serde_json::json!([]));
}

#[test]
fn config_show_gives_the_same_output_every_time() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    for args in [&["config", "show"][..], &["--json", "config", "show"][..]] {
        let first = sandbox.run_in(&fake.root, args);
        let second = sandbox.run_in(&fake.root, args);
        assert_eq!(first.code, Some(0), "{first:?}");
        assert_eq!(first.stdout, second.stdout, "{args:?}");
    }
}

/// A pack of the repository is listed with the config as its origin.
#[test]
fn config_show_lists_a_pack_of_the_repository_as_coming_from_the_config() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();
    write(
        &root,
        ".gsttt/packs/mine.toml",
        "name = \"mine\"\nlanguage = \"ts\"\ndetect = [\"mine-package\"]\n",
    );

    let shown = json(&sandbox.run_in(&root, &["--json", "config", "show"]));

    let packs = shown["packs"].as_array().unwrap();
    assert!(
        packs.iter().any(|pack| pack["name"] == "mine"
            && pack["language"] == "ts"
            && pack["origin"] == "config"),
        "{shown}"
    );
}
