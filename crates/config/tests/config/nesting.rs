//! The nesting limit, through the whole way from the file to the setup, on a thread with a small stack.

use std::fmt::Write as _;
use std::thread;

use gsttt_core::Setup;
use serde_json::{Value, json};

use crate::tree::{Tree, evaluate_at};

/// The stack of the thread that reads the config. A config nested a hundred thousand deep would overflow it in the parser, in serde or in the `Drop` of the value, so that only the limit, applied before any value is built, keeps these tests alive.
const SMALL_STACK: usize = 256 * 1024;

/// Evaluates the tree on a thread with [`SMALL_STACK`].
fn evaluate_tree_on_a_small_stack(tree: &Tree) -> Result<Setup, String> {
    let root = tree.root().to_path_buf();
    thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(move || evaluate_at(&root).map_err(|error| error.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

/// Evaluates `config` as `.gsttt/config.toml` on a thread with [`SMALL_STACK`].
fn evaluate_on_a_small_stack(config: &str) -> Result<Setup, String> {
    evaluate_tree_on_a_small_stack(&Tree::with_config(config))
}

/// The message of a config that must be refused for its nesting.
fn refusal(config: &str) -> String {
    evaluate_on_a_small_stack(config).unwrap_err()
}

#[gsttt_scenario::scenario("config", "Deep nesting is refused")]
#[test]
fn deep_nesting_is_refused_with_the_place_where_the_limit_is_passed() {
    // A hundred thousand inline arrays, closed again, so that the file is valid TOML and only its depth is wrong.
    let depth = 100_000;
    let arrays = format!("[ts]\nx = {}1{}\n", "[".repeat(depth), "]".repeat(depth));
    let message = refusal(&arrays);
    // `[ts]` is the first level, so the 64th bracket, at the 68th column of the second line, is the one that passes 64.
    assert!(
        message.starts_with(".gsttt/config.toml:2:68: "),
        "{message}"
    );
    assert!(message.contains("limit of 64"), "{message}");

    // Eighty inline tables whose keys have eighty dotted parts each: within the limits of the parser one by one, and
    // 6400 levels together.
    let key = vec!["a"; 80].join(".");
    let open = format!("{{ {key} = ");
    let tables = format!("[ts]\nx = {}1{}\n", open.repeat(80), " }".repeat(80));
    let message = refusal(&tables);
    assert!(message.starts_with(".gsttt/config.toml:2:"), "{message}");
    assert!(message.contains("limit of 64"), "{message}");
}

#[gsttt_scenario::scenario("config", "Nesting at the limit is read")]
#[test]
fn nesting_at_the_limit_is_read() {
    // The table `[ts]` is the first level and the 63 arrays are the other 63.
    let config = format!("[ts]\nx = {}1{}\n", "[".repeat(63), "]".repeat(63));
    let setup = evaluate_on_a_small_stack(&config).unwrap();
    let nested = (0..63).fold(json!(1), |inner, _| Value::Array(vec![inner]));
    assert_eq!(setup.options("ts"), &json!({ "x": nested }));

    // One array more is past the limit.
    let past = format!("[ts]\nx = {}1{}\n", "[".repeat(64), "]".repeat(64));
    assert!(refusal(&past).starts_with(".gsttt/config.toml:2:68: "));
}

#[gsttt_scenario::scenario("config", "Arrays of tables count twice")]
#[test]
fn arrays_of_tables_count_twice() {
    // `[[ts.k1]]`, `[[ts.k1.k2]]`, … down to a header of 40 parts: no part is more than 40 deep ...
    let mut headers = String::new();
    for count in 1..40 {
        let names: Vec<String> = (1..=count).map(|index| format!("k{index}")).collect();
        writeln!(headers, "[[ts.{}]]", names.join(".")).unwrap();
    }
    assert_eq!(headers.lines().last().unwrap().matches('.').count(), 39);

    // ... but `ts` is one level and each name after it is an array of tables, which is two: the header of 32 names
    // passes 64, on the 32nd line.
    let message = refusal(&headers);
    assert!(message.starts_with(".gsttt/config.toml:32:"), "{message}");
    assert!(message.contains("limit of 64"), "{message}");

    // The header before it, of 31 names, is the last that fits: that it is the arrays that count twice, and not the length of the headers, is what stops the 32nd.
    let fewer = headers.lines().take(31).collect::<Vec<_>>().join("\n");
    evaluate_on_a_small_stack(&fewer).unwrap();
}

#[test]
fn a_pack_file_is_held_to_the_same_limit() {
    let tree = Tree::empty();
    let depth = 100_000;
    tree.write(
        ".gsttt/packs/deep.toml",
        &format!(
            "name = \"deep\"\nlanguage = \"ts\"\nx = {}1{}\n",
            "[".repeat(depth),
            "]".repeat(depth)
        ),
    );
    let message = evaluate_tree_on_a_small_stack(&tree).unwrap_err();
    assert!(
        message.starts_with(".gsttt/packs/deep.toml:3:"),
        "{message}"
    );
    assert!(message.contains("limit of 64"), "{message}");
}
