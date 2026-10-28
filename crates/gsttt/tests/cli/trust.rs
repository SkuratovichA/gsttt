//! An external connector runs only in a repository where the person trusted it.
//!
//! Each repository declares the external `fake`, whose program creates a marker file and ends, so a marker says the
//! connector was started. The runs share one sandbox, and so one trust store, which is the sandbox's and never the
//! real one of the user.

use std::path::Path;

use gsttt_scenario::scenario;

use super::support::{FAKE_CONFIG, FAKE_CONFIG_PATH, Fake, Ran, Sandbox, is_empty, json, write};

/// The coverage problem of an external that is not trusted.
const NOT_TRUSTED: &str = "the external connector `fake` (/bin/sh fake.sh) is not trusted in this repository; run `gsttt config trust` in a terminal to allow it";

/// Indexes `root`, which must end well.
fn index(sandbox: &Sandbox, root: &Path) {
    let index = sandbox.run_in(root, &["index"]);
    assert_eq!(index.code, Some(0), "{index:?}");
}

/// Answers the question of `gsttt config trust` in `root` with `typed`, on a terminal.
fn trust(sandbox: &Sandbox, root: &Path, typed: &str) -> Ran {
    sandbox.run_typing(root, &["config", "trust"], typed)
}

/// Trusts `fake` in its repository, and shows that it is trusted: the next index starts it and leaves the marker,
/// which is removed again.
fn trusted(sandbox: &Sandbox, fake: &Fake) {
    let granted = trust(sandbox, &fake.root, "y\n");
    assert_eq!(granted.code, Some(0), "{granted:?}");
    index(sandbox, &fake.root);
    assert!(
        fake.marker.exists(),
        "a trusted connector is started: {granted:?}"
    );
    std::fs::remove_file(&fake.marker).unwrap();
}

/// What `gsttt coverage` says in `root`: it names the external that is not trusted, and counts its files as
/// unclaimed.
fn assert_names_the_untrusted(sandbox: &Sandbox, root: &Path) {
    let coverage = sandbox.run_in(root, &["coverage"]);
    assert_eq!(coverage.code, Some(0), "{coverage:?}");
    assert!(coverage.stdout.contains(NOT_TRUSTED), "{coverage:?}");
    assert!(coverage.stdout.contains(".fake  1 file"), "{coverage:?}");
}

/// The text of the config of `fake` with `cmd` changed to add an argument.
fn config_with_argument() -> String {
    let edited = FAKE_CONFIG.replace("\"fake.sh\"]", "\"fake.sh\", \"--again\"]");
    assert_ne!(edited, FAKE_CONFIG);
    edited
}

/// The config of `fake` declared `count` times, as `fake00`, `fake01` and so on.
fn config_with_externals(count: usize) -> String {
    let declared: String = (0..count)
        .map(|n| FAKE_CONFIG.replace("name = \"fake\"", &format!("name = \"fake{n:02}\"")))
        .collect();
    assert_ne!(declared, FAKE_CONFIG.repeat(count));
    declared
}

#[scenario("cli", "An untrusted external is not started")]
#[test]
fn an_untrusted_external_is_not_started() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    index(&sandbox, &fake.root);

    assert!(!fake.marker.exists(), "the connector was started");
    assert_names_the_untrusted(&sandbox, &fake.root);
}

#[scenario("cli", "Trust belongs to one repository")]
#[test]
fn trust_belongs_to_one_repository() {
    let sandbox = Sandbox::new();
    let first = sandbox.fake_external("first");
    let second = sandbox.fake_external("second");
    trusted(&sandbox, &first);

    index(&sandbox, &second.root);

    assert!(!second.marker.exists(), "the second repository started it");
    assert_names_the_untrusted(&sandbox, &second.root);
}

#[scenario("cli", "Editing the command takes trust away")]
#[test]
fn editing_the_command_takes_trust_away() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    trusted(&sandbox, &fake);

    write(&fake.root, FAKE_CONFIG_PATH, &config_with_argument());
    index(&sandbox, &fake.root);

    assert!(!fake.marker.exists(), "the connector was started");
    let coverage = sandbox.run_in(&fake.root, &["coverage"]);
    assert!(
        coverage.stdout.contains(
            "the external connector `fake` (/bin/sh fake.sh --again) is not trusted in this repository; run `gsttt config trust` in a terminal to allow it"
        ),
        "{coverage:?}"
    );
}

#[scenario("cli", "Editing the program takes trust away")]
#[test]
fn editing_the_program_takes_trust_away() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    trusted(&sandbox, &fake);

    let script = std::fs::read_to_string(fake.root.join("fake.sh")).unwrap();
    write(&fake.root, "fake.sh", &format!("{script}# a new line\n"));
    index(&sandbox, &fake.root);

    assert!(!fake.marker.exists(), "the connector was started");
    assert_names_the_untrusted(&sandbox, &fake.root);
}

#[scenario("cli", "Untrust forgets the repository")]
#[test]
fn untrust_forgets_the_repository() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    trusted(&sandbox, &fake);

    let untrusted = sandbox.run_in(&fake.root, &["config", "untrust"]);
    assert_eq!(untrusted.code, Some(0), "{untrusted:?}");
    index(&sandbox, &fake.root);

    assert!(!fake.marker.exists(), "the connector was started");
    assert_names_the_untrusted(&sandbox, &fake.root);
}

#[scenario("cli", "A trusted external is started")]
#[test]
fn a_trusted_external_is_started() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    let granted = trust(&sandbox, &fake.root, "y\n");
    assert_eq!(granted.code, Some(0), "{granted:?}");
    index(&sandbox, &fake.root);

    assert!(fake.marker.exists(), "the connector was not started");
    // The person was shown what would run, and where, before the question.
    let asked = format!(
        "fake: /bin/sh fake.sh\nRun these commands whenever gsttt indexes {}? [y/N] ",
        fake.root.display()
    );
    assert!(granted.stderr.contains(&asked), "{granted:?}");
    assert_eq!(granted.stdout, "", "{granted:?}");
}

#[scenario("cli", "Trust needs a terminal")]
#[test]
fn trust_needs_a_terminal() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    let refused = sandbox.run_piped(&fake.root, &["config", "trust"], "y\n");

    assert_eq!(refused.code, Some(2), "{refused:?}");
    assert!(refused.stderr.contains("needs a terminal"), "{refused:?}");
    assert_eq!(refused.stdout, "", "{refused:?}");
    assert!(is_empty(sandbox.state.path()), "something was stored");
    index(&sandbox, &fake.root);
    assert!(!fake.marker.exists(), "the connector was started");
}

#[scenario("cli", "Declining trusts nothing")]
#[test]
fn declining_trusts_nothing() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    let declined = trust(&sandbox, &fake.root, "n\n");

    assert_eq!(declined.code, Some(0), "{declined:?}");
    assert!(
        declined.stderr.contains("fake: /bin/sh fake.sh"),
        "{declined:?}"
    );
    assert!(is_empty(sandbox.state.path()), "something was stored");
    index(&sandbox, &fake.root);
    assert!(!fake.marker.exists(), "the connector was started");
}

/// The document of `gsttt coverage --json` holds the problems of the index under one key: the report of the engine holds them, and the command adds none of its own.
#[test]
fn the_coverage_document_holds_problems_once() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");

    let coverage = sandbox.run_in(&fake.root, &["coverage", "--json"]);

    assert_eq!(coverage.code, Some(0), "{coverage:?}");
    assert_eq!(coverage.stdout.matches("\"problems\":").count(), 1);
    let document = json(&coverage);
    assert_eq!(document["problems"], serde_json::json!([NOT_TRUSTED]));
}

/// The coverage names every external that waits for trust. Its other lists stop at 30 entries, and the line that tells a person to run `gsttt config trust` must not be the one left out.
#[test]
fn coverage_names_every_external_that_waits_for_trust() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    write(&fake.root, FAKE_CONFIG_PATH, &config_with_externals(31));

    let coverage = sandbox.run_in(&fake.root, &["coverage"]);

    assert_eq!(coverage.code, Some(0), "{coverage:?}");
    assert!(
        coverage.stdout.starts_with("problems (31):\n"),
        "{coverage:?}"
    );
    for n in 0..31 {
        let line = NOT_TRUSTED.replace("`fake`", &format!("`fake{n:02}`"));
        assert!(
            coverage.stdout.contains(&format!("\n  {line}\n")),
            "{n}: {coverage:?}"
        );
    }
    assert!(!coverage.stdout.contains("… "), "{coverage:?}");
}

/// Only `y` and `yes` grant trust. An empty answer, which is the default of the question, and any other word
/// decline it, and so does a capital letter.
#[test]
fn only_yes_grants_trust() {
    for typed in ["\n", "no\n", "yep\n", "Y\n", "YES\n", "sure\n"] {
        let sandbox = Sandbox::new();
        let fake = sandbox.fake_external("repo");

        let declined = trust(&sandbox, &fake.root, typed);

        assert_eq!(declined.code, Some(0), "{typed:?}: {declined:?}");
        assert!(is_empty(sandbox.state.path()), "{typed:?}: stored");
    }
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    let granted = trust(&sandbox, &fake.root, " yes \n");
    assert_eq!(granted.code, Some(0), "{granted:?}");
    assert!(!is_empty(sandbox.state.path()), "{granted:?}");
}

/// With nothing waiting for trust, `gsttt config trust` says so and asks nothing.
#[test]
fn trust_with_nothing_to_trust_says_so() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository();

    let nothing = trust(&sandbox, &root, "y\n");

    assert_eq!(nothing.code, Some(0), "{nothing:?}");
    assert!(!nothing.stderr.contains("[y/N]"), "{nothing:?}");
    assert!(nothing.stderr.contains("nothing to trust"), "{nothing:?}");
    assert!(is_empty(sandbox.state.path()), "something was stored");
}

/// An external trusted once is not asked about again, and trusting twice stores one line.
#[test]
fn trusting_twice_stores_one_line() {
    let sandbox = Sandbox::new();
    let fake = sandbox.fake_external("repo");
    trusted(&sandbox, &fake);

    let again = trust(&sandbox, &fake.root, "y\n");

    assert_eq!(again.code, Some(0), "{again:?}");
    assert!(again.stderr.contains("nothing to trust"), "{again:?}");
    let store = std::fs::read_to_string(sandbox.state.path().join("gsttt/trust")).unwrap();
    assert_eq!(store.lines().count(), 1, "{store:?}");
    assert!(
        store.ends_with(&format!(" {}\n", fake.root.display())),
        "{store:?}"
    );
}
