//! Raw SQL of a Prisma project, read through the whole pipeline: the TypeScript connector finds the document, the `sql` connector reads it, and the `prisma` connector reads the schema.

use gsttt_scenario::scenario;

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
