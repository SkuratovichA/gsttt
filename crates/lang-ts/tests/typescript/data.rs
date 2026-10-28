//! Tables and environment variables.

use std::error::Error;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use gsttt_scenario::scenario;
use serde_json::Value;

use crate::support::{NOTHING, contracts, run};

/// Runs `fixture` on another thread and gives up after `limit`, so that a walk that does not end fails its test instead of hanging the run.
fn run_within(fixture: &'static str, limit: Duration) -> Result<Value, Box<dyn Error>> {
    let (sender, receiver) = mpsc::channel();
    // The receiver is gone when the test gave up, and then there is nobody to tell.
    thread::spawn(move || {
        sender
            .send(run(fixture).map_err(|error| error.to_string()))
            .ok()
    });
    Ok(receiver.recv_timeout(limit)??)
}

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
    let json = run_within("prisma-alias-walk", Duration::from_secs(20))?;
    assert_eq!(contracts(&json, "src/chain.ts"), ["calls table tenant @14"]);
    assert_eq!(
        contracts(&json, "src/cycle.ts"),
        ["calls table booking @13"]
    );
    assert_eq!(contracts(&json, "src/fan-out.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A name bound to anything else is no alias of this")]
fn a_name_bound_to_anything_else_is_no_alias_of_this() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-mixed-aliases")?;
    assert_eq!(contracts(&json, "src/self-alias.ts"), NOTHING);
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
    assert_eq!(
        contracts(&json, "src/consistent.ts"),
        ["calls table tenant @4", "calls table booking @8"]
    );
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
#[scenario("typescript", "A renamed or combined client type is no root")]
fn a_renamed_or_combined_client_type_is_no_root() -> Result<(), Box<dyn Error>> {
    let json = run("prisma-unread-types")?;
    for path in ["src/renamed.ts", "src/namespaced.ts", "src/intersected.ts"] {
        assert_eq!(contracts(&json, path), NOTHING, "{path}");
    }
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
