//! The contract of an index source, shown on the fake that every other test builds on.

use gsttt_mcp::IndexSource;

use crate::fake::Fake;

fn fake() -> Fake {
    Fake::small()
}

#[test]
fn the_fake_counts_each_kind_of_call() {
    let source = fake();

    source.refreshed().unwrap();
    source.stored().unwrap();
    let target = source.path("./lib/../lib/util.ts").unwrap();

    assert_eq!(target.path.as_str(), "lib/util.ts");
    assert!(target.on_disk);
    assert_eq!(source.refreshes(), 1);
    assert_eq!(source.stored_reads(), 1);
    assert_eq!(source.paths_read(), ["./lib/../lib/util.ts"]);
    assert_eq!(source.calls(), 3);
    assert_eq!(source.peak(), 1);
}

#[test]
fn a_clone_shares_the_counters_of_the_source_it_came_from() {
    let source = fake();
    let given_away = source.clone();

    given_away.refreshed().unwrap();

    assert_eq!(source.refreshes(), 1);
}

#[test]
fn a_failing_source_fails_every_call_with_its_message_and_counts_it() {
    let source = fake().failing("the index is locked by another process");

    let errors = [
        source.refreshed().unwrap_err(),
        source.stored().unwrap_err(),
        source.path("src/a.ts").unwrap_err(),
        source.changed(None).unwrap_err(),
    ];

    for error in errors {
        assert_eq!(error.message, "the index is locked by another process");
    }
    assert_eq!(source.calls(), 4);
    assert_eq!(source.refreshes(), 0);
}

#[test]
fn a_path_outside_the_root_is_an_error_that_says_so() {
    let error = fake().path("../elsewhere.ts").unwrap_err();

    assert!(
        error.message.contains("is outside the repository"),
        "{error}"
    );
}
