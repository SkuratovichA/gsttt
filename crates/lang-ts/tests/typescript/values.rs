//! Names that another file declares, followed into the text a rule reads: in a template or a concatenation, in a constant built from them, and in the base of a client.

use std::error::Error;
use std::fmt::Write;
use std::fs;
use std::time::Duration;

use gsttt_scenario::scenario;

use crate::support::{
    NOTHING, contracts, linked, problems, run, run_tree_within, run_with_pack, run_within, sites,
    unread,
};

/// The number of files in the diamond of constants, each of which names the one before it twice.
const DIAMOND_LEVELS: usize = 24;

/// The number of constants in the chain across two files, which a read that calls itself once for each of them could not follow on the stack of a worker.
const CHAIN_LENGTH: usize = 4000;

/// The number of constants in `deep.ts` that each name the one before: more than a part may nest, and more than the link data of a file may hold if they all were kept.
const DEEP_LEVELS: usize = 60;

/// The number of constants in the doubling, each of which names the one before it twice: the last would read as a few gigabytes of text.
const DOUBLING_STEPS: usize = 30;

/// A rule that serves the method a decorator is on, behind the prefix the decorator itself gives.
const ROUTE_PACK: &str = r#"name = "fixture"
language = "ts"

[[decorators]]
decorator = "Route"
kind = "http"
role = "serves"
key = "$member"
method = "GET"
prefix = "$arg1"
"#;

/// More than a read of a constant takes, and less than a read that visits the constants of a cycle once for every way round.
const CYCLE_LIMIT: Duration = Duration::from_secs(20);

#[test]
#[scenario("typescript", "A template with an imported constant is read")]
fn a_template_with_an_imported_constant_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("values-template")?;
    assert_eq!(
        linked(&json),
        [
            "calls http * /api/v1/users @3 in src/users.ts",
            "calls http * /api/v1/orders @4 in src/users.ts",
        ]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An exported constant built from an imported one is read"
)]
fn an_exported_constant_built_from_an_imported_one_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("values-exported")?;
    assert_eq!(
        linked(&json),
        [
            "calls http * /api/v1/users @3 in src/client.ts",
            "calls http * /api/v1/orders @4 in src/client.ts",
            "calls http * /api/v1/items @5 in src/client.ts",
            "calls http * /api/v1/users/1 @6 in src/client.ts",
        ]
    );
    assert_eq!(contracts(&json, "src/client.ts"), NOTHING);
    assert_eq!(unread(&json, "src/client.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A constant of the same file built from an imported one is read"
)]
fn a_constant_of_the_same_file_built_from_an_imported_one_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("values-same-file")?;
    assert_eq!(
        linked(&json),
        [
            "calls http * /api/v1/users @7 in src/users.ts",
            "calls http * /api/v1/orders @8 in src/users.ts",
            "calls http * /api/v1/users/1 @9 in src/users.ts",
            "calls http * /api/v1 @10 in src/users.ts",
        ]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A client made with an imported base URL is called")]
fn a_client_made_with_an_imported_base_url_is_called() -> Result<(), Box<dyn Error>> {
    let json = run("values-client")?;
    assert_eq!(
        linked(&json),
        [
            "calls http GET /api/v1/users @6 in src/users.ts",
            "calls http POST /api/v1/orders @7 in src/users.ts",
        ]
    );
    assert_eq!(
        sites("values-client")?,
        [
            "calls http GET /api/v1/users @6 in src/users.ts",
            "calls http POST /api/v1/orders @7 in src/users.ts",
        ]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An imported name the link cannot read keeps its reading"
)]
fn an_imported_name_the_link_cannot_read_keeps_its_reading() -> Result<(), Box<dyn Error>> {
    let json = run("values-unreadable")?;
    // The template and the client read the name as nothing, as the parse did before the link was asked; the two bare names stay unread.
    let read = [
        "calls http * /users @5 in src/users.ts",
        "calls http GET /orders @10 in src/users.ts",
    ];
    assert_eq!(linked(&json), read);
    assert_eq!(
        sites("values-unreadable")?,
        ["calls env API_URL @1 in src/env.ts", read[0], read[1]]
    );
    assert_eq!(contracts(&json, "src/users.ts"), NOTHING);
    // `PATH` is a constant that names an import, and the link finds nothing to read it with: `BASE` is no text in `env.ts`.
    assert_eq!(
        unread(&json, "src/users.ts"),
        ["@6 non-literal fetch", "@7 non-literal fetch"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A name that a local binds again is no import")]
fn a_name_that_a_local_binds_again_is_no_import() -> Result<(), Box<dyn Error>> {
    let json = run("values-shadow")?;
    // The parameter hides the import in its function, and the file is not read by scope, so the import is read nowhere in it.
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(
        contracts(&json, "src/users.ts"),
        ["calls http * /users @4", "calls http * /orders @7",]
    );
    assert_eq!(unread(&json, "src/users.ts"), ["@8 non-literal fetch"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_constant_names_an_import_written_below_it() -> Result<(), Box<dyn Error>> {
    let json = run("values-order")?;
    assert_eq!(
        linked(&json),
        [
            "calls http * /api/v1/users @4 in src/users.ts",
            "calls http * /root/orders @5 in src/users.ts",
        ]
    );
    assert_eq!(unread(&json, "src/users.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A cycle of constants stays unread")]
fn a_cycle_of_constants_stays_unread() -> Result<(), Box<dyn Error>> {
    let json = run("values-cycle")?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(contracts(&json, "src/use.ts"), NOTHING);
    assert_eq!(unread(&json, "src/use.ts"), ["@3 non-literal fetch"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_cycle_of_constants_that_name_each_other_many_times_ends() -> Result<(), Box<dyn Error>> {
    let json = run_within("values-cycle-wide", CYCLE_LIMIT)?;
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(unread(&json, "src/use.ts"), ["@3 non-literal fetch"]);
    Ok(())
}

#[test]
fn a_diamond_of_constants_across_many_files_is_read_once_per_constant() -> Result<(), Box<dyn Error>>
{
    let tree = tempfile::tempdir()?;
    fs::create_dir(tree.path().join("src"))?;
    fs::write(tree.path().join("package.json"), r#"{ "name": "diamond" }"#)?;
    fs::write(
        tree.path().join("src/l0.ts"),
        "export const A = '/api';\nexport const B = '';\n",
    )?;
    // Each level reads both constants of the one before it, and each of those reads both of the one before it: a read that starts again at every name visits a constant of the first level 2^24 times.
    for level in 1..=DIAMOND_LEVELS {
        let mut source = String::new();
        writeln!(
            source,
            "import {{ A as PA, B as PB }} from './l{}';",
            level - 1
        )?;
        writeln!(source, "export const A = `${{PA}}${{PB}}`;")?;
        writeln!(source, "export const B = `${{PB}}${{PB}}`;")?;
        fs::write(tree.path().join(format!("src/l{level}.ts")), source)?;
    }
    fs::write(
        tree.path().join("src/use.ts"),
        format!("import {{ A }} from './l{DIAMOND_LEVELS}';\nfetch(`${{A}}/users`);\n"),
    )?;
    let json = run_tree_within(tree.path(), CYCLE_LIMIT)?;
    assert_eq!(linked(&json), ["calls http * /api/users @2 in src/use.ts"]);
    assert_eq!(unread(&json, "src/use.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_cycle_of_constants_is_unread_from_every_constant_that_reaches_it() -> Result<(), Box<dyn Error>>
{
    let tree = tempfile::tempdir()?;
    fs::create_dir(tree.path().join("src"))?;
    fs::write(tree.path().join("package.json"), r#"{ "name": "cycle" }"#)?;
    for (file, source) in [
        (
            "a",
            "import { B } from './b';\nexport const A = `${B}/x`;\n",
        ),
        (
            "b",
            "import { A } from './a';\nexport const B = `${A}/y`;\n",
        ),
        (
            "c",
            "import { B } from './b';\nexport const C = `${B}/z`;\n",
        ),
        (
            "use",
            "import { A } from './a';\nimport { B } from './b';\nimport { C } from './c';\nfetch(C);\nfetch(B);\nfetch(A);\n",
        ),
    ] {
        fs::write(tree.path().join(format!("src/{file}.ts")), source)?;
    }
    let json = run_tree_within(tree.path(), CYCLE_LIMIT)?;
    // What a constant reads as is kept once it is read, and it is the same from either end of the cycle and from a constant outside it.
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(
        unread(&json, "src/use.ts"),
        [
            "@4 non-literal fetch",
            "@5 non-literal fetch",
            "@6 non-literal fetch",
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A chain of constants across two files is read to its end"
)]
fn a_chain_of_constants_across_two_files_is_read_to_its_end() -> Result<(), Box<dyn Error>> {
    let tree = tempfile::tempdir()?;
    fs::create_dir(tree.path().join("src"))?;
    fs::write(tree.path().join("package.json"), r#"{ "name": "chain" }"#)?;
    // `K<n>` names `K<n+1>`, which the other of the two files keeps, and the last is text.
    for (file, other, parity) in [("a", "b", 0), ("b", "a", 1)] {
        let mine = || (0..CHAIN_LENGTH).filter(|number| number % 2 == parity);
        let taken: Vec<String> = mine()
            .filter(|number| number + 1 < CHAIN_LENGTH)
            .map(|number| format!("K{}", number + 1))
            .collect();
        let mut source = format!("import {{ {} }} from './{other}';\n", taken.join(", "));
        for number in mine() {
            if number + 1 < CHAIN_LENGTH {
                writeln!(source, "export const K{number} = `${{K{}}}/y`;", number + 1)?;
            } else {
                writeln!(source, "export const K{number} = '/end';")?;
            }
        }
        fs::write(tree.path().join(format!("src/{file}.ts")), source)?;
    }
    fs::write(
        tree.path().join("src/use.ts"),
        "import { K0 } from './a';\nfetch(K0);\n",
    )?;
    let json = run_tree_within(tree.path(), CYCLE_LIMIT)?;
    assert_eq!(
        linked(&json),
        [format!(
            "calls http * /end{} @2 in src/use.ts",
            "/y".repeat(CHAIN_LENGTH - 1)
        )]
    );
    assert_eq!(unread(&json, "src/use.ts"), NOTHING);
    assert_eq!(problems(&json), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A constant built deeper than the limit is no constant and the project still links"
)]
fn a_constant_built_deeper_than_the_limit_is_no_constant_and_the_project_still_links()
-> Result<(), Box<dyn Error>> {
    let tree = tempfile::tempdir()?;
    fs::create_dir(tree.path().join("src"))?;
    fs::write(tree.path().join("package.json"), r#"{ "name": "deep" }"#)?;
    fs::write(
        tree.path().join("src/base.ts"),
        "export const BASE = '/api';\n",
    )?;
    // `D<n>` nests `n + 1` levels, so `D31` is the last that is 32 levels deep.
    let mut deep = String::from("import { BASE } from './base';\n");
    writeln!(deep, "export const D0 = `${{BASE}}/x`;")?;
    for level in 1..DEEP_LEVELS {
        writeln!(deep, "export const D{level} = `${{D{}}}/x`;", level - 1)?;
    }
    writeln!(deep, "fetch(D31);")?;
    writeln!(deep, "fetch(D{});", DEEP_LEVELS - 1)?;
    fs::write(tree.path().join("src/deep.ts"), deep)?;
    fs::write(
        tree.path().join("src/other.ts"),
        "import { BASE } from './base';\nfetch(`${BASE}/users`);\n",
    )?;
    let json = run_tree_within(tree.path(), CYCLE_LIMIT)?;
    let mut found = linked(&json);
    found.sort();
    assert_eq!(
        found,
        [
            "calls http * /api/users @2 in src/other.ts".to_owned(),
            format!(
                "calls http * /api{} @{} in src/deep.ts",
                "/x".repeat(32),
                DEEP_LEVELS + 2
            ),
        ]
    );
    assert_eq!(
        unread(&json, "src/deep.ts"),
        [format!("@{} non-literal fetch", DEEP_LEVELS + 3)]
    );
    assert_eq!(problems(&json), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A constant that doubles past the limit is no constant and the index completes"
)]
fn a_constant_that_doubles_past_the_limit_is_no_constant_and_the_index_completes()
-> Result<(), Box<dyn Error>> {
    let tree = tempfile::tempdir()?;
    fs::create_dir(tree.path().join("src"))?;
    fs::write(
        tree.path().join("package.json"),
        r#"{ "name": "doubling" }"#,
    )?;
    fs::write(tree.path().join("src/i.ts"), "export const I = '/xyz';\n")?;
    let mut big = String::from("import { I } from './i';\n");
    writeln!(big, "export const A0 = `${{I}}${{I}}`;")?;
    for step in 1..DOUBLING_STEPS {
        writeln!(
            big,
            "export const A{step} = `${{A{}}}${{A{}}}`;",
            step - 1,
            step - 1
        )?;
    }
    fs::write(tree.path().join("src/big.ts"), big)?;
    fs::write(
        tree.path().join("src/use.ts"),
        format!(
            "import {{ A1, A{last} }} from './big';\nfetch(A1);\nfetch(A{last});\n",
            last = DOUBLING_STEPS - 1
        ),
    )?;
    let json = run_tree_within(tree.path(), CYCLE_LIMIT)?;
    assert_eq!(
        linked(&json),
        ["calls http * /xyz/xyz/xyz/xyz @2 in src/use.ts"]
    );
    assert_eq!(unread(&json, "src/use.ts"), ["@3 non-literal fetch"]);
    assert_eq!(problems(&json), NOTHING);
    Ok(())
}

#[test]
fn a_method_that_is_an_imported_name_defers_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("values-method")?;
    // A site that waited for the link and stayed unread would be in the `unread` list of the link, which the index has only when the link returned some.
    assert_eq!(json.get("unread"), None);
    assert_eq!(linked(&json), NOTHING);
    assert_eq!(contracts(&json, "src/call.ts"), NOTHING);
    assert_eq!(unread(&json, "src/call.ts"), ["@3 non-literal fetch"]);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A decorator prefix that is an imported name is read by the link"
)]
fn a_decorator_prefix_that_is_an_imported_name_is_read_by_the_link() -> Result<(), Box<dyn Error>> {
    let json = run_with_pack("values-prefix", ROUTE_PACK)?;
    assert_eq!(
        linked(&json),
        ["serves http GET /api/v1/list @4 in src/routes.ts"]
    );
    assert_eq!(contracts(&json, "src/routes.ts"), NOTHING);
    assert_eq!(unread(&json, "src/routes.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A method that is an imported name is read by the link")]
fn a_method_that_is_an_imported_name_is_read_by_the_link() -> Result<(), Box<dyn Error>> {
    let json = run("values-verb")?;
    assert_eq!(linked(&json), ["calls http POST /orders @3 in src/call.ts"]);
    assert_eq!(contracts(&json, "src/call.ts"), NOTHING);
    assert_eq!(unread(&json, "src/call.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}
