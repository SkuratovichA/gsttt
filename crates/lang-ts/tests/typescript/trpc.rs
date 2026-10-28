//! tRPC procedures and the calls that name them.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{contracts, linked, run};

#[test]
#[scenario("typescript", "Composed routers serve full paths")]
fn composed_routers_serve_full_paths() -> Result<(), Box<dyn Error>> {
    let json = run("trpc-routers")?;
    assert_eq!(
        linked(&json),
        ["serves trpc booking.get @7 in src/booking.ts"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A client call names its procedure")]
fn a_client_call_names_its_procedure() -> Result<(), Box<dyn Error>> {
    let json = run("trpc-call")?;
    assert_eq!(
        contracts(&json, "src/Bookings.tsx"),
        ["calls trpc booking.get @4"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}
