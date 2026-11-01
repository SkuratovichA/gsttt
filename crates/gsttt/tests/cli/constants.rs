//! A constant that a file imports is read through the link, so a change to the file that exports it reaches the file that imports it.

use serde_json::Value;

use super::support::{Sandbox, json, write};

/// The paths of the files that call `key`, from the answer of `contract`.
fn callers(sandbox: &Sandbox, key: &str) -> Vec<String> {
    let run = sandbox.run(&["--json", "contract", key]);
    assert_eq!(run.code, Some(0), "{run:?}");
    json(&run)["found"]["calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| call["path"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_changed_constant_of_an_exporter_moves_the_key_of_an_importer_that_is_reused() {
    let sandbox = Sandbox::new();
    let dir = sandbox.work.path();
    write(dir, "src/paths.ts", "export const USERS = '/users';\n");
    write(
        dir,
        "src/client.ts",
        "import { USERS } from './paths';\nfetch(USERS);\n",
    );

    let first = sandbox.run(&["index"]);
    assert_eq!(first.code, Some(0), "{first:?}");
    assert!(
        first.stdout.contains(": 2 parsed, 0 reused.\n"),
        "{first:?}"
    );
    assert_eq!(callers(&sandbox, "http * /users"), ["src/client.ts"]);

    write(dir, "src/paths.ts", "export const USERS = '/people';\n");
    let second = sandbox.run(&["index"]);
    assert_eq!(second.code, Some(0), "{second:?}");
    // Only the exporter is parsed again: the importer is reused, and its key follows the constant because the link reads it.
    assert!(
        second.stdout.contains(": 1 parsed, 1 reused.\n"),
        "{second:?}"
    );
    assert_eq!(callers(&sandbox, "http * /people"), ["src/client.ts"]);

    let stale = sandbox.run(&["--json", "contract", "http * /users"]);
    assert_eq!(stale.code, Some(0), "{stale:?}");
    assert_eq!(
        json(&stale)["found"]["lookup"],
        Value::from("search"),
        "the old key is gone: {stale:?}"
    );
}
