//! Tables and environment variables.

use std::error::Error;
use std::fmt::{self, Write};
use std::fs;
use std::time::Duration;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, run, run_tree_within, run_within};

#[test]
#[scenario("typescript", "A Prisma call names its table")]
fn a_prisma_call_names_its_table() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-call")?;
    assert_eq!(
        contracts(&json, "src/bookings.ts"),
        ["calls table bookingitem @5"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An alias of this reaches the Prisma client")]
fn an_alias_of_this_reaches_the_prisma_client() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-this-alias")?;
    assert_eq!(
        contracts(&json, "src/tenants.ts"),
        ["calls table tenant @9", "calls table booking @15"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A parameter typed as a pick of the client is a root")]
fn a_parameter_typed_as_a_pick_of_the_client_is_a_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-picked-client")?;
    assert_eq!(
        contracts(&json, "src/tenants.ts"),
        ["calls table tenant @4", "calls table tenantbranch @5"]
    );
    assert_eq!(contracts(&json, "src/tenants.test.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn every_wrapper_of_a_client_type_keeps_the_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-wrapped-clients")?;
    // One file per wrapper of the list in `roots.rs`, and one for a union with null, so that dropping an entry of the list fails the assertion of its own file.
    for path in [
        "src/pick.ts",
        "src/omit.ts",
        "src/partial.ts",
        "src/required.ts",
        "src/readonly.ts",
        "src/non-nullable.ts",
        "src/nullable.ts",
    ] {
        assert_eq!(contracts(&json, path), ["calls table tenant @4"], "{path}");
    }
    // The wrapper keeps the first type argument, so a pick of a type that is no root is no root.
    assert_eq!(contracts(&json, "src/other.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A local type alias of a client type is a root")]
fn a_local_type_alias_of_a_client_type_is_a_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-type-alias")?;
    assert_eq!(
        contracts(&json, "src/readers.ts"),
        ["calls table tenant @10"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A long chain of type aliases is followed to its end")]
fn a_long_chain_of_type_aliases_is_followed_to_its_end() -> Result<(), Box<dyn Error>> {
    let json = run_within("prisma-alias-walk", ALIAS_WALK_LIMIT)?;
    assert_eq!(contracts(&json, "src/chain.ts"), ["calls table tenant @14"]);
    assert_eq!(
        contracts(&json, "src/cycle.ts"),
        ["calls table booking @13"]
    );
    assert_eq!(contracts(&json, "src/fan-out.ts"), NOTHING);
    insta::assert_json_snapshot!(json);

    // Far more aliases than a stack of 2 MiB holds frames of a walk that recurses on each of them.
    let tree = tempfile::tempdir()?;
    fs::create_dir(tree.path().join("src"))?;
    fs::write(
        tree.path().join("package.json"),
        r#"{ "name": "alias-chain", "dependencies": { "@prisma/client": "^6.0.0" } }"#,
    )?;
    fs::write(
        tree.path().join("src/long.ts"),
        long_chain(LONG_CHAIN_LENGTH)?,
    )?;
    let json = run_tree_within(tree.path(), ALIAS_WALK_LIMIT)?;
    assert_eq!(
        contracts(&json, "src/long.ts"),
        [format!("calls table tenant @{}", LONG_CHAIN_LENGTH + 3)]
    );
    Ok(())
}

/// How long the walk over the aliases of a file may take before the test gives up on it.
const ALIAS_WALK_LIMIT: Duration = Duration::from_secs(20);

/// The number of aliases in the generated chain.
const LONG_CHAIN_LENGTH: usize = 100_000;

/// A file of `length` type aliases, the first of which is `PrismaClient` and each of the others the one before it, and a function whose parameter has the last, and which calls a table on line `length + 3`.
fn long_chain(length: usize) -> Result<String, fmt::Error> {
    let mut source = String::from(
        "import type { PrismaClient } from '@prisma/client';\ntype Chain1 = PrismaClient;\n",
    );
    for index in 2..=length {
        writeln!(source, "type Chain{index} = Chain{};", index - 1)?;
    }
    writeln!(
        source,
        "export function find(client: Chain{length}) {{\n  return client.tenant.findFirst();\n}}"
    )?;
    Ok(source)
}

#[test]
#[scenario("typescript", "A name bound to anything else is no alias of this")]
fn a_name_bound_to_anything_else_is_no_alias_of_this() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-mixed-aliases")?;
    assert_eq!(contracts(&json, "src/self-alias.ts"), NOTHING);
    assert_eq!(contracts(&json, "src/earlier.ts"), NOTHING);
    assert_eq!(
        contracts(&json, "src/consistent.ts"),
        ["calls table tenant @8", "calls table booking @13"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A name declared with another type is no root")]
fn a_name_declared_with_another_type_is_no_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-mixed-types")?;
    assert_eq!(contracts(&json, "src/names.ts"), NOTHING);
    assert_eq!(contracts(&json, "src/properties.ts"), NOTHING);
    assert_eq!(contracts(&json, "src/earlier.ts"), NOTHING);
    assert_eq!(
        contracts(&json, "src/consistent.ts"),
        ["calls table tenant @4", "calls table booking @8"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A binding made with new has the type it constructs")]
fn a_binding_made_with_new_has_the_type_it_constructs() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-constructed-clients")?;
    assert_eq!(contracts(&json, "src/seed.ts"), ["calls table tenant @6"]);
    assert_eq!(contracts(&json, "src/alone.ts"), ["calls table booking @6"]);
    assert_eq!(
        contracts(&json, "src/members.ts"),
        ["calls table invoice @7", "calls table payment @12"]
    );
    assert_eq!(contracts(&json, "src/other.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A signature without a body binds nothing")]
fn a_signature_without_a_body_binds_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-signatures")?;
    assert_eq!(contracts(&json, "src/signatures.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A function type binds nothing wherever it is written")]
fn a_function_type_binds_nothing_wherever_it_is_written() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-function-types")?;
    assert_eq!(contracts(&json, "src/types.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A signature is no reason for another declaration to be no root"
)]
fn a_signature_is_no_reason_for_another_declaration_to_be_no_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-signature-names")?;
    let expected: Vec<String> = (42..=50)
        .map(|line| format!("calls table tenant @{line}"))
        .collect();
    assert_eq!(contracts(&json, "src/shared.ts"), expected);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A local type shadows a root type")]
fn a_local_type_shadows_a_root_type() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-local-types")?;
    for path in [
        "src/alias.ts",
        "src/interface.ts",
        "src/class.ts",
        "src/enum.ts",
    ] {
        assert_eq!(contracts(&json, path), NOTHING, "{path}");
    }
    assert_eq!(
        contracts(&json, "src/alias-of-root.ts"),
        ["calls table tenant @6"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A client type not written by its name is no root")]
fn a_client_type_not_written_by_its_name_is_no_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-unread-types")?;
    for path in [
        "src/renamed.ts",
        "src/namespaced.ts",
        "src/typed-of.ts",
        "src/aliased-import.ts",
    ] {
        assert_eq!(contracts(&json, path), NOTHING, "{path}");
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An intersection with a client type is a root")]
fn an_intersection_with_a_client_type_is_a_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-intersected-types")?;
    assert_eq!(
        contracts(&json, "src/intersected.ts"),
        ["calls table tenant @4"]
    );
    assert_eq!(contracts(&json, "src/unrelated.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A class or an interface that extends a client type is a root"
)]
fn a_class_or_an_interface_that_extends_a_client_type_is_a_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-extended-types")?;
    assert_eq!(
        contracts(&json, "src/service.ts"),
        ["calls table tenant @13"]
    );
    assert_eq!(
        contracts(&json, "src/chain.ts"),
        [
            "calls table tenant @12",
            "calls table tenant @16",
            "calls table tenant @20",
            "calls table tenant @24",
            "calls table tenant @30",
        ]
    );
    assert_eq!(contracts(&json, "src/unrelated.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A client typed as PrismaClient or PrismaService is a root"
)]
fn a_client_typed_as_prisma_client_or_prisma_service_is_a_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-class-roots")?;
    assert_eq!(
        contracts(&json, "src/invoices.ts"),
        [
            "calls table invoice @12",
            "calls table payment @16",
            "calls table booking @25",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An imported model type calls its table")]
fn an_imported_model_type_calls_its_table() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-model-imports")?;
    assert_eq!(
        contracts(&json, "src/models.ts"),
        [
            "calls table tenant @1",
            "calls table tenantbranch @1",
            "calls table bookingstatus @7",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A re-exported model type calls its table")]
fn a_re_exported_model_type_calls_its_table() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-model-reexport")?;
    assert_eq!(contracts(&json, "src/models.ts"), ["calls table tenant @1"]);
    assert_eq!(
        contracts(&json, "src/renamed.ts"),
        ["calls table tenant @1"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "An import type reaches the import rules")]
fn an_import_type_reaches_the_import_rules() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-import-type")?;
    assert_eq!(
        contracts(&json, "src/types.ts"),
        ["calls table tenant @1", "calls table tenantbranch @2"]
    );
    assert_eq!(
        contracts(&json, "src/declared.ts"),
        ["calls table tenant @1"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A namespace import of the client names nothing")]
fn a_namespace_import_of_the_client_names_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-namespace-import")?;
    assert_eq!(contracts(&json, "src/client.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A model type taken from another package names nothing")]
fn a_model_type_taken_from_another_package_names_nothing() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-other-packages")?;
    for path in ["src/custom-output.ts", "src/shared.ts"] {
        assert_eq!(contracts(&json, path), NOTHING, "{path}");
    }
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "ORM declarations declare tables")]
fn orm_declarations_declare_tables() -> Result<(), Box<dyn Error>> {
    let json = run("orm-declarations")?;
    assert_eq!(
        contracts(&json, "src/booking.entity.ts"),
        ["declares table bookings @3"]
    );
    assert_eq!(
        contracts(&json, "src/room.entity.ts"),
        ["declares table room @3"]
    );
    assert_eq!(
        contracts(&json, "src/schema.ts"),
        ["declares table booking_items @3"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "Every way of reading the environment is seen")]
fn every_way_of_reading_the_environment_is_seen() -> Result<(), Box<dyn Error>> {
    let json = run("environment")?;
    assert_eq!(
        contracts(&json, "src/config.ts"),
        [
            "calls env A @1",
            "calls env B @2",
            "calls env VITE_C @3",
            "calls env D @4",
            "calls env E @4",
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
fn a_prisma_client_named_db_or_tx_names_its_table() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-roots")?;
    let facts = contracts(&json, "src/repo.ts");
    assert!(
        facts
            .iter()
            .any(|fact| fact.starts_with("calls table invoice @4")),
        "{facts:?}"
    );
    assert!(
        facts
            .iter()
            .any(|fact| fact.starts_with("calls table payment @5")),
        "{facts:?}"
    );
    Ok(())
}
