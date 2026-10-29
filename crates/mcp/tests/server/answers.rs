//! What a tool answers: the text the command line prints for the same question, from the index the source refreshed.

use gsttt_core::Freshness;
use gsttt_core::engine::changes::ChangeKind;
use gsttt_core::query::{self, Answer, Entries, ImpactRequest};
use gsttt_core::testing::Tree;
use gsttt_core::text::thousands;
use gsttt_mcp::BUDGET;
use gsttt_scenario::scenario;
use serde_json::json;

use crate::fake::Fake;
use crate::session::Session;

/// A tree in which `count` files import `lib/util.ts`, each from a path `padding` characters longer than `src/user00.ts`.
fn imported_by(count: usize, padding: usize) -> Fake {
    let tree = (0..count).fold(Tree::new(), |tree, number| {
        let importer = format!("src/user{number:02}{}.ts", "x".repeat(padding));
        tree.imports(&importer, "lib/util.ts")
    });
    Fake::over(tree.build())
}

/// The first line of an answer that was cut to fit the budget.
fn cut_line() -> String {
    format!(
        "cut: lists shortened to fit this answer in {} characters; the \"more\" counts say what they leave out. Ask a narrower question for the rest: fewer files, or a smaller depth.",
        thousands(BUDGET)
    )
}

/// The text of `answer` cut to `shown` entries of each list, after the cut line.
fn cut_to(answer: &impl Answer, shown: usize) -> String {
    format!("{}\n{}", cut_line(), answer.text(Entries::First(shown)))
}

fn size(text: &str) -> usize {
    text.chars().count()
}

/// The importers of `lib/util.ts` a text lists.
fn importers(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("src/user"))
        .count()
}

/// What the command line prints when it is asked about `files` and `deleted`.
fn impact_text(source: &Fake, request: &ImpactRequest, entries: Entries) -> String {
    query::impact(&source.index(), Freshness::Fresh, request).text(entries)
}

#[scenario("mcp", "A path argument is read from the root")]
#[tokio::test]
async fn a_path_argument_is_read_from_the_root() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("dependents", json!({ "path": "lib/util.ts" }))
        .await;

    assert!(!reply.is_error, "{}", reply.text);
    assert_eq!(source.paths_read(), ["lib/util.ts"]);
    assert!(
        reply
            .text
            .starts_with("dependents of lib/util.ts (1):\nsrc/a.ts"),
        "{}",
        reply.text
    );
}

#[tokio::test]
async fn a_path_is_asked_about_as_the_source_read_it() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("dependents", json!({ "path": "./src/../lib/util.ts" }))
        .await;

    assert_eq!(source.paths_read(), ["./src/../lib/util.ts"]);
    let expected = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts");
    assert_eq!(reply.text, expected.text(Entries::Capped));
}

#[scenario("mcp", "A tool answers with the CLI's text")]
#[tokio::test]
async fn a_tool_answers_with_one_block_of_the_text_the_cli_prints() {
    let source = Fake::over(
        Tree::new()
            .imports("src/a.ts", "lib/util.ts")
            .imports("src/b.ts", "lib/util.ts")
            .build(),
    );
    let session = Session::start(source.clone()).await;

    // `Session::call` fails unless the result holds exactly one text block.
    let reply = session
        .call("dependents", json!({ "path": "lib/util.ts" }))
        .await;

    let expected = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts");
    assert!(!reply.is_error);
    assert_eq!(reply.text, expected.to_string());
    assert!(reply.text.contains("src/a.ts") && reply.text.contains("src/b.ts"));
    assert_eq!(source.refreshes(), 1);
}

#[tokio::test]
async fn each_tool_answers_with_the_text_of_its_query() {
    let source = Fake::over(
        Tree::new()
            .imports("src/a.ts", "lib/util.ts")
            .serves("src/server.ts", "http GET /bookings/{}")
            .calls("src/a.ts", "http GET /bookings/{}")
            .build(),
    );
    let index = source.index();
    let session = Session::start(source.clone()).await;
    let fresh = Freshness::Fresh;
    let request = ImpactRequest {
        files: vec!["lib/util.ts".to_owned()],
        ..ImpactRequest::default()
    };
    let cases = [
        (
            "impact",
            json!({ "paths": ["lib/util.ts"] }),
            query::impact(&index, fresh, &request).to_string(),
        ),
        (
            "dependents",
            json!({ "path": "lib/util.ts" }),
            query::dependents(&index, fresh, "lib/util.ts").to_string(),
        ),
        (
            "dependencies",
            json!({ "path": "src/a.ts" }),
            query::dependencies(&index, fresh, "src/a.ts").to_string(),
        ),
        (
            "path",
            json!({ "from": "src/a.ts", "to": "lib/util.ts" }),
            query::path(&index, fresh, "src/a.ts", "lib/util.ts").to_string(),
        ),
        (
            "contract",
            json!({ "query": "http GET /bookings/{}" }),
            query::contract(&index, fresh, "http GET /bookings/{}")
                .unwrap()
                .to_string(),
        ),
        (
            "coverage",
            json!({}),
            query::coverage(&index, fresh).to_string(),
        ),
    ];

    for (position, (tool, arguments, expected)) in cases.into_iter().enumerate() {
        let reply = session.call(tool, arguments).await;

        assert!(!reply.is_error, "{tool}: {}", reply.text);
        assert_eq!(reply.text, expected, "{tool}");
        assert_eq!(source.refreshes(), position + 1, "{tool} refreshes once");
    }
    let contract = session
        .call("contract", json!({ "query": "http GET /bookings/{}" }))
        .await;
    assert!(
        contract.text.contains("src/server.ts") && contract.text.contains("src/a.ts"),
        "the contract is found, not missed: {}",
        contract.text
    );
}

#[scenario("mcp", "all lists every entry")]
#[tokio::test]
async fn all_lists_every_entry() {
    let source = imported_by(45, 0);
    let session = Session::start(source.clone()).await;

    let all = session
        .call("dependents", json!({ "path": "lib/util.ts", "all": true }))
        .await;
    let cut = session
        .call("dependents", json!({ "path": "lib/util.ts" }))
        .await;

    let answer = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts");
    assert_eq!(all.text, answer.text(Entries::All));
    assert_eq!(all.text.matches("static import").count(), 45);
    assert_eq!(cut.text, answer.text(Entries::Capped));
    assert_eq!(cut.text.matches("static import").count(), query::LIST_CAP);
}

#[scenario("mcp", "An answer over the budget is cut to fit")]
#[tokio::test]
async fn an_answer_over_the_budget_is_cut_to_fit() {
    let source = imported_by(600, 90);
    let session = Session::start(source.clone()).await;

    let dependents = session
        .call("dependents", json!({ "path": "lib/util.ts", "all": true }))
        .await;
    let impact = session
        .call("impact", json!({ "paths": ["lib/util.ts"], "all": true }))
        .await;

    for (tool, reply) in [("dependents", &dependents), ("impact", &impact)] {
        assert!(!reply.is_error, "{tool}: {}", reply.text);
        assert!(
            size(&reply.text) <= BUDGET,
            "{tool} holds {} characters",
            size(&reply.text)
        );
        assert!(
            reply.text.starts_with(&cut_line()),
            "{tool}: {}",
            reply.text.chars().take(200).collect::<String>()
        );
        assert!(importers(&reply.text) > query::LIST_CAP, "{tool}");
        assert!(
            reply
                .text
                .lines()
                .any(|line| line.starts_with("… ") && line.ends_with(" more")),
            "{tool} counts the rest"
        );
    }
    let answer = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts");
    let shown = importers(&dependents.text);
    assert_eq!(dependents.text, cut_to(&answer, shown));
    assert!(
        size(&cut_to(&answer, shown + 1)) > BUDGET,
        "one importer more does not fit"
    );
}

#[scenario("mcp", "A cut list is cut further when it does not fit")]
#[tokio::test]
async fn a_cut_list_is_cut_further_when_it_does_not_fit() {
    let source = imported_by(45, 900);
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("dependents", json!({ "path": "lib/util.ts" }))
        .await;

    let answer = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts");
    assert!(
        size(&answer.text(Entries::Capped)) > BUDGET,
        "the first 30 importers do not fit"
    );
    assert!(
        size(&reply.text) <= BUDGET,
        "{} characters",
        size(&reply.text)
    );
    assert!(reply.text.starts_with(&cut_line()));
    let shown = importers(&reply.text);
    assert!(shown < query::LIST_CAP, "{shown} importers");
    assert_eq!(reply.text, cut_to(&answer, shown));
    assert!(
        size(&cut_to(&answer, shown + 1)) > BUDGET,
        "one importer more does not fit"
    );
}

#[tokio::test]
async fn names_that_alone_fill_the_budget_are_cut_and_every_row_stays() {
    let owned: Vec<String> = (0..400)
        .map(|number| format!("a_name_of_some_length_{number:03}"))
        .collect();
    let names: Vec<&str> = owned.iter().map(String::as_str).collect();
    let tree = (0..3).fold(Tree::new(), |tree, number| {
        tree.imports_names(&format!("src/user{number}.ts"), "lib/util.ts", &names)
    });
    let source = Fake::over(tree.build());
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("dependents", json!({ "path": "lib/util.ts", "all": true }))
        .await;

    let answer = query::dependents(&source.index(), Freshness::Fresh, "lib/util.ts");
    assert!(size(&answer.text(Entries::All)) > BUDGET);
    // The three rows stay, with the names of each cut to three and a count.
    assert_eq!(
        reply.text,
        format!("{}\n{}", cut_line(), answer.text(Entries::Capped))
    );
}

#[tokio::test]
async fn problems_that_alone_fill_the_budget_are_returned_whole_after_the_cut_line() {
    let mut index = Tree::new().imports("src/a.ts", "lib/util.ts").build();
    index.coverage.problems.push("p".repeat(BUDGET + 1));
    let source = Fake::over(index);
    let session = Session::start(source.clone()).await;

    let reply = session.call("coverage", json!({})).await;

    // The problems of a coverage answer are never cut, so no list is left to shorten.
    let answer = query::coverage(&source.index(), Freshness::Fresh);
    assert_eq!(reply.text, cut_to(&answer, 0));
    assert!(size(&reply.text) > BUDGET);
}

#[tokio::test]
async fn all_does_not_change_an_answer_that_is_cut_before_it_reaches_the_cap() {
    let session = Session::start(imported_by(45, 900)).await;

    let capped = session
        .call("dependents", json!({ "path": "lib/util.ts" }))
        .await;
    let all = session
        .call("dependents", json!({ "path": "lib/util.ts", "all": true }))
        .await;

    assert!(capped.text.starts_with(&cut_line()));
    assert_eq!(all.text, capped.text);
}

/// A tree whose every list is longer than the cut: 45 importers of a file, a file that imports 45, 45 served routes, 45 skipped files, and a chain of 45 hops from `src/chain00.ts` to `src/chain45.ts`.
fn long_lists() -> Fake {
    let tree = (0..45).fold(Tree::new().skipped(45), |tree, number| {
        tree.imports(&format!("src/user{number:02}.ts"), "lib/util.ts")
            .imports("src/hub.ts", &format!("lib/dep{number:02}.ts"))
            .imports(
                &format!("src/chain{number:02}.ts"),
                &format!("src/chain{:02}.ts", number + 1),
            )
            .serves(
                &format!("src/route{number:02}.ts"),
                &format!("http GET /items/{number:02}"),
            )
    });
    Fake::over(tree.build())
}

#[tokio::test]
async fn all_reaches_every_tool_that_takes_it() {
    let source = long_lists();
    let session = Session::start(source.clone()).await;
    let cases = [
        (
            "impact",
            json!({ "paths": ["lib/util.ts"] }),
            "src/user44.ts",
        ),
        (
            "dependents",
            json!({ "path": "lib/util.ts" }),
            "src/user44.ts",
        ),
        (
            "dependencies",
            json!({ "path": "src/hub.ts" }),
            "lib/dep44.ts",
        ),
        (
            "path",
            json!({ "from": "src/chain00.ts", "to": "src/chain45.ts" }),
            "src/chain44.ts",
        ),
        (
            "contract",
            json!({ "query": "/items" }),
            "http GET /items/44",
        ),
        ("coverage", json!({}), "assets/skipped44.bin"),
    ];

    for (tool, mut arguments, last) in cases {
        let cut = session.call(tool, arguments.clone()).await;
        arguments["all"] = json!(true);
        let all = session.call(tool, arguments).await;

        assert!(!cut.is_error && !all.is_error, "{tool}: {}", cut.text);
        // The lists are sorted, so the last entry is the first to be cut.
        assert!(
            !cut.text.contains(last),
            "{tool} cuts its lists: {}",
            cut.text
        );
        assert!(
            all.text.contains(last),
            "{tool} lists every entry: {}",
            all.text
        );
    }
}

#[tokio::test]
async fn a_path_with_no_file_on_disk_is_asked_about_as_deleted() {
    let source = Fake::small().not_on_disk("lib/util.ts");
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("impact", json!({ "paths": ["lib/util.ts", "src/a.ts"] }))
        .await;

    let deleted = ImpactRequest {
        files: vec!["src/a.ts".to_owned()],
        deleted: vec!["lib/util.ts".to_owned()],
        ..ImpactRequest::default()
    };
    let named = ImpactRequest {
        files: vec!["lib/util.ts".to_owned(), "src/a.ts".to_owned()],
        ..ImpactRequest::default()
    };
    assert_eq!(reply.text, impact_text(&source, &deleted, Entries::Capped));
    assert_ne!(reply.text, impact_text(&source, &named, Entries::Capped));
}

#[tokio::test]
async fn changed_asks_about_the_files_git_lists() {
    let source = Fake::small().changes(&[
        ("lib/util.ts", ChangeKind::Modified),
        ("src/new.ts", ChangeKind::Added),
        ("src/gone.ts", ChangeKind::Deleted),
    ]);
    let session = Session::start(source.clone()).await;

    let reply = session.call("impact", json!({ "changed": true })).await;

    let request = ImpactRequest {
        files: vec!["lib/util.ts".to_owned(), "src/new.ts".to_owned()],
        deleted: vec!["src/gone.ts".to_owned()],
        ..ImpactRequest::default()
    };
    assert_eq!(reply.text, impact_text(&source, &request, Entries::Capped));
    assert_eq!(source.bases_read(), [None]);
    assert!(
        source.paths_read().is_empty(),
        "no path is read when git lists the files"
    );
}

#[tokio::test]
async fn base_is_handed_to_the_source() {
    let source = Fake::small().changes(&[("lib/util.ts", ChangeKind::Modified)]);
    let session = Session::start(source.clone()).await;

    let reply = session
        .call("impact", json!({ "changed": true, "base": "main" }))
        .await;

    assert!(!reply.is_error, "{}", reply.text);
    assert_eq!(source.bases_read(), [Some("main".to_owned())]);
}

#[tokio::test]
async fn depth_limits_the_walk() {
    let tree = Tree::new()
        .imports("src/b.ts", "src/a.ts")
        .imports("src/c.ts", "src/b.ts")
        .imports("src/d.ts", "src/c.ts");
    let source = Fake::over(tree.build());
    let session = Session::start(source.clone()).await;

    let near = session
        .call("impact", json!({ "paths": ["src/a.ts"], "depth": 1 }))
        .await;
    let far = session
        .call("impact", json!({ "paths": ["src/a.ts"] }))
        .await;

    let one_hop = ImpactRequest {
        files: vec!["src/a.ts".to_owned()],
        depth: Some(1),
        ..ImpactRequest::default()
    };
    let every_hop = ImpactRequest {
        files: vec!["src/a.ts".to_owned()],
        ..ImpactRequest::default()
    };
    assert_eq!(near.text, impact_text(&source, &one_hop, Entries::Capped));
    assert_eq!(far.text, impact_text(&source, &every_hop, Entries::Capped));
    assert_ne!(near.text, far.text);
}

#[scenario("mcp", "A file the index does not hold is a tool error")]
#[tokio::test]
async fn a_file_the_index_does_not_hold_is_a_tool_error() {
    let source = Fake::small();
    let index = source.index();
    let session = Session::start(source).await;
    let fresh = Freshness::Fresh;
    let cases = [
        (
            "dependents",
            json!({ "path": "src/unknown.ts" }),
            query::dependents(&index, fresh, "src/unknown.ts").text(Entries::Capped),
        ),
        (
            "dependencies",
            json!({ "path": "src/unknown.ts" }),
            query::dependencies(&index, fresh, "src/unknown.ts").text(Entries::Capped),
        ),
        (
            "path",
            json!({ "from": "src/a.ts", "to": "src/unknown.ts" }),
            query::path(&index, fresh, "src/a.ts", "src/unknown.ts").text(Entries::Capped),
        ),
    ];

    for (tool, arguments, printed) in cases {
        let reply = session.call(tool, arguments).await;

        assert!(reply.is_error, "{tool}: {}", reply.text);
        assert_eq!(reply.text, printed, "{tool}");
        assert!(
            reply.text.starts_with("src/unknown.ts is not in the index"),
            "{tool}: {}",
            reply.text
        );
    }
}

#[tokio::test]
async fn a_chain_that_does_not_exist_between_files_the_index_holds_is_an_answer() {
    let source = Fake::small();
    let session = Session::start(source.clone()).await;

    // The files are held and nothing leads from the one to the other, which is what the answer says.
    let reply = session
        .call("path", json!({ "from": "lib/util.ts", "to": "src/a.ts" }))
        .await;

    let printed = query::path(&source.index(), Freshness::Fresh, "lib/util.ts", "src/a.ts");
    assert!(!reply.is_error, "{}", reply.text);
    assert_eq!(reply.text, printed.text(Entries::Capped));
    assert!(reply.text.starts_with("no chain from"), "{}", reply.text);
}
