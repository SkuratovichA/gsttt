//! Raw SQL of a Prisma project, read through the whole pipeline: the TypeScript connector finds the document, the `sql` connector reads it, and the `prisma` connector reads the schema.

use gsttt_scenario::scenario;

use serde_json::Value;

use super::support::{Sandbox, json, write};

fn tree(sandbox: &Sandbox) {
    let dir = sandbox.work.path();
    write(
        dir,
        "package.json",
        r#"{ "name": "api", "dependencies": { "@prisma/client": "^6.0.0" } }"#,
    );
    write(
        dir,
        "prisma/schema.prisma",
        "model User {\n  id Int @id\n}\n",
    );
    write(
        dir,
        "src/model.ts",
        "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nexport const all = () => prisma.user.findMany();\n",
    );
    write(
        dir,
        "src/quoted.ts",
        "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nexport const one = (id: number) => prisma.$queryRaw`SELECT id FROM \"User\" WHERE id = ${id}`;\n",
    );
    write(
        dir,
        "src/qualified.ts",
        "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nexport const two = () => prisma.$queryRaw`SELECT id FROM public.\"User\"`;\n",
    );
    write(
        dir,
        "src/fragment.ts",
        "import { Prisma } from '@prisma/client';\nexport const part = (id: number) => Prisma.sql`AND id = ${id}`;\n",
    );
}

/// The `(path, via)` of every site of `table user`, by role, from the answer of `contract`.
fn sites(sandbox: &Sandbox, role: &str) -> Vec<(String, String)> {
    let run = sandbox.run(&["--json", "contract", "table user"]);
    json(&run)["found"][role]
        .as_array()
        .map(|sites| {
            sites
                .iter()
                .map(|site| {
                    (
                        site["path"].as_str().unwrap_or_default().to_owned(),
                        site["via"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn site(path: &str, via: &str) -> (String, String) {
    (path.to_owned(), via.to_owned())
}

#[test]
#[scenario(
    "typescript",
    "A raw query, a model call and a Prisma model name one table"
)]
fn a_raw_query_a_model_call_and_a_prisma_model_name_one_table() {
    let sandbox = Sandbox::new();
    tree(&sandbox);
    assert_eq!(
        sites(&sandbox, "declares"),
        [site("prisma/schema.prisma", "prisma")]
    );
    // `"User"` and `public."User"` are the key of `model User`, in lower case like every table key.
    assert_eq!(
        sites(&sandbox, "calls"),
        [
            site("src/model.ts", "prisma"),
            site("src/qualified.ts", "sql"),
            site("src/quoted.ts", "sql"),
        ]
    );
}

#[test]
#[scenario(
    "typescript",
    "A Prisma.sql fragment that is no statement names no table"
)]
fn a_prisma_sql_fragment_that_is_no_statement_names_no_table() {
    let sandbox = Sandbox::new();
    tree(&sandbox);
    let calls = sites(&sandbox, "calls");
    assert!(
        calls.iter().all(|(path, _)| path != "src/fragment.ts"),
        "{calls:?}"
    );
}

fn driver_tree(sandbox: &Sandbox) {
    let dir = sandbox.work.path();
    write(
        dir,
        "package.json",
        r#"{ "name": "api", "dependencies": { "pg": "^8.0.0" } }"#,
    );
    write(
        dir,
        "src/users.ts",
        "declare const db: { query: (text: string) => unknown };\n\nexport const find = () => db.query('SELECT id FROM users');\n",
    );
    write(
        dir,
        "src/sessions.ts",
        "declare const pool: { query: (text: string) => unknown };\n\nexport const clear = () => pool.query('DELETE FROM sessions');\n",
    );
    write(
        dir,
        "src/template.ts",
        "declare const pool: { query: (text: string) => unknown };\n\nexport const order = (id: number) => pool.query(`SELECT id FROM orders WHERE id = ${id}`);\n\nexport const any = (table: string) => pool.query(`SELECT id FROM ${table}`);\n",
    );
    write(
        dir,
        "src/shell.ts",
        "declare const cp: { exec: (command: string) => unknown };\n\nexport const list = () => cp.exec('ls -la');\n",
    );
}

/// Every table key of the tree, sorted, from the answer of `contract table`.
fn tables(sandbox: &Sandbox) -> Vec<String> {
    let run = sandbox.run(&["--json", "contract", "table"]);
    let mut keys: Vec<String> = json(&run)["found"]["matches"]
        .as_array()
        .map(|matches| {
            matches
                .iter()
                .map(|found| found["key"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default();
    keys.sort();
    keys
}

/// The paths that call `key`, from the answer of `contract`.
fn callers(sandbox: &Sandbox, key: &str) -> Vec<String> {
    let run = sandbox.run(&["--json", "contract", key]);
    json(&run)["found"]["calls"]
        .as_array()
        .map(|sites| {
            sites
                .iter()
                .map(|site| site["path"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The unread sites of the TypeScript connector, from the answer of `coverage`.
fn unread_sites(sandbox: &Sandbox) -> Vec<Value> {
    let run = sandbox.run(&["--json", "coverage"]);
    json(&run)["connectors"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|connector| connector["connector"] == "ts")
        .flat_map(|connector| {
            connector["unread_sites"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

#[test]
#[scenario("typescript", "A driver query calls the tables it names")]
fn a_driver_query_calls_the_tables_it_names() {
    let sandbox = Sandbox::new();
    driver_tree(&sandbox);
    assert_eq!(callers(&sandbox, "table users"), ["src/users.ts"]);
    assert_eq!(callers(&sandbox, "table sessions"), ["src/sessions.ts"]);
}

#[test]
#[scenario("typescript", "A computed table names no table")]
fn a_computed_table_names_no_table() {
    let sandbox = Sandbox::new();
    driver_tree(&sandbox);
    // `orders` is the one table of `src/template.ts`; `SELECT id FROM ?` adds none.
    assert_eq!(
        tables(&sandbox),
        ["table orders", "table sessions", "table users"]
    );
    assert_eq!(callers(&sandbox, "table orders"), ["src/template.ts"]);
    assert_eq!(unread_sites(&sandbox), Vec::<Value>::new());
}

#[test]
#[scenario("typescript", "A literal that is no SQL names no table")]
fn a_literal_that_is_no_sql_names_no_table() {
    let sandbox = Sandbox::new();
    driver_tree(&sandbox);
    assert_eq!(
        tables(&sandbox),
        ["table orders", "table sessions", "table users"]
    );
    for key in tables(&sandbox) {
        assert!(
            !callers(&sandbox, &key).contains(&"src/shell.ts".to_owned()),
            "{key}"
        );
    }
    assert_eq!(unread_sites(&sandbox), Vec::<Value>::new());
}
