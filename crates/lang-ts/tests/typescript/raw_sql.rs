//! Raw SQL that Prisma runs: documents embedded in tagged templates and in the arguments of the `Unsafe` forms.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, embedded, provides, run, unread};

#[test]
#[scenario(
    "typescript",
    "A raw query template is read as SQL with its computed parts written as placeholders"
)]
fn a_raw_query_template_is_read_as_sql_with_its_computed_parts_written_as_placeholders()
-> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql")?;
    let documents = embedded(&json, "src/queries.ts");
    assert_eq!(
        documents.first().map(String::as_str),
        Some("sql @5 SELECT id FROM a WHERE id = ?")
    );
    assert!(
        documents.contains(&"sql @8 SELECT id FROM b WHERE id IN (?, ?) LIMIT ?".to_owned()),
        "{documents:?}"
    );
    assert_eq!(unread(&json, "src/queries.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A raw query template with a type argument is read as SQL"
)]
fn a_raw_query_template_with_a_type_argument_is_read_as_sql() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql")?;
    let documents = embedded(&json, "src/queries.ts");
    assert!(
        documents.contains(&"sql @11 SELECT id FROM c WHERE id = ?::uuid".to_owned()),
        "{documents:?}"
    );
    assert!(
        documents.contains(&"sql @13 DELETE FROM d".to_owned()),
        "{documents:?}"
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A raw query on a client held by this is read as SQL")]
fn a_raw_query_on_a_client_held_by_this_is_read_as_sql() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql")?;
    assert_eq!(
        embedded(&json, "src/service.ts"),
        [
            "sql @7 SELECT id FROM reports",
            "sql @11 SELECT id FROM audits"
        ]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An unsafe raw query with a literal is read as SQL")]
fn an_unsafe_raw_query_with_a_literal_is_read_as_sql() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-unsafe")?;
    let documents = embedded(&json, "src/unsafe.ts");
    assert_eq!(
        &documents[..2],
        ["sql @5 SELECT id FROM a", "sql @7 UPDATE b SET seen = true"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An unsafe raw query with a computed text is unread")]
fn an_unsafe_raw_query_with_a_computed_text_is_unread() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-unsafe")?;
    assert_eq!(
        unread(&json, "src/unsafe.ts"),
        [
            "@9 non-literal prisma.$queryRawUnsafe",
            "@14 non-literal prisma.$executeRawUnsafe"
        ]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "An unsafe raw query with a constant text is read as SQL"
)]
fn an_unsafe_raw_query_with_a_constant_text_is_read_as_sql() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-unsafe")?;
    assert_eq!(
        embedded(&json, "src/constant.ts"),
        ["sql @7 SELECT id FROM f"]
    );
    assert_eq!(unread(&json, "src/constant.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A Prisma.sql template held in a name is read where it stands"
)]
fn a_prisma_sql_template_held_in_a_name_is_read_where_it_stands() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-unsafe")?;
    assert_eq!(embedded(&json, "src/held.ts"), ["sql @5 SELECT id FROM e"]);
    assert_eq!(unread(&json, "src/held.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A Prisma.sql template passed to a raw query is read once"
)]
fn a_prisma_sql_template_passed_to_a_raw_query_is_read_once() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-unsafe")?;
    let documents = embedded(&json, "src/unsafe.ts");
    assert_eq!(documents.len(), 3, "{documents:?}");
    assert_eq!(documents[2], "sql @12 SELECT id FROM c WHERE id = ?");
    Ok(())
}

#[test]
#[scenario("typescript", "A Prisma.sql template is read as SQL whole or in part")]
fn a_prisma_sql_template_is_read_as_sql_whole_or_in_part() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-sql-tag")?;
    assert_eq!(
        embedded(&json, "src/fragments.ts"),
        ["sql @3 SELECT id FROM a WHERE id = ?", "sql @5 AND id = ?"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A raw query is read only in a project that depends on the Prisma client"
)]
fn a_raw_query_is_read_only_in_a_project_that_depends_on_the_prisma_client()
-> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql-no-client")?;
    assert_eq!(embedded(&json, "src/queries.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "The prisma pack is credited with the SQL it finds")]
fn the_prisma_pack_is_credited_with_the_sql_it_finds() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-raw-sql")?;
    assert!(
        provides(&json, ".").contains(&"table calls via prisma".to_owned()),
        "{:?}",
        provides(&json, ".")
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A driver query is read as SQL")]
fn a_driver_query_is_read_as_sql() -> Result<(), Box<dyn Error>> {
    let json = run("sql-drivers")?;
    assert_eq!(
        embedded(&json, "src/users.ts"),
        [
            "sql @4 SELECT id FROM users WHERE id = $1",
            "sql @8 DELETE FROM sessions"
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A query kept in a constant is read")]
fn a_query_kept_in_a_constant_is_read() -> Result<(), Box<dyn Error>> {
    let json = run("sql-drivers")?;
    assert_eq!(
        embedded(&json, "src/constant.ts"),
        ["sql @5 SELECT id FROM accounts"]
    );
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A call with no text in its first argument records nothing"
)]
fn a_call_with_no_text_in_its_first_argument_records_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("sql-drivers")?;
    assert_eq!(embedded(&json, "src/silent.ts"), NOTHING);
    assert_eq!(unread(&json, "src/silent.ts"), NOTHING);
    Ok(())
}

#[test]
fn a_literal_that_is_no_sql_is_read_and_unread_nowhere() -> Result<(), Box<dyn Error>> {
    let json = run("sql-drivers")?;
    assert_eq!(embedded(&json, "src/shell.ts"), ["sql @3 ls -la"]);
    assert_eq!(unread(&json, "src/shell.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A query template is read with its computed parts as placeholders"
)]
fn a_query_template_is_read_with_its_computed_parts_as_placeholders() -> Result<(), Box<dyn Error>>
{
    let json = run("sql-drivers")?;
    assert_eq!(
        embedded(&json, "src/template.ts"),
        [
            "sql @3 SELECT id FROM orders WHERE id = ?",
            "sql @5 SELECT id FROM ?"
        ]
    );
    assert_eq!(unread(&json, "src/template.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A project without a SQL driver reads no query")]
fn a_project_without_a_sql_driver_reads_no_query() -> Result<(), Box<dyn Error>> {
    let json = run("sql-drivers-absent")?;
    assert_eq!(embedded(&json, "src/users.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "The sql-drivers pack is credited with the tables it finds"
)]
fn the_sql_drivers_pack_is_credited_with_the_tables_it_finds() -> Result<(), Box<dyn Error>> {
    let json = run("sql-drivers")?;
    assert!(
        provides(&json, ".").contains(&"table calls via sql-drivers".to_owned()),
        "{:?}",
        provides(&json, ".")
    );
    Ok(())
}
