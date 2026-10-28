//! Tables and environment variables.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{contracts, run};

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
