//! tRPC procedures and the calls that name them.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{contracts, linked, run, unread};

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

#[test]
#[scenario("typescript", "A procedure held in a variable is served")]
fn a_procedure_held_in_a_variable_is_served() -> Result<(), Box<dyn Error>> {
    let json = run("trpc-refs")?;
    let mut served = linked(&json);
    served.sort();
    assert_eq!(
        served,
        [
            "serves trpc inline @12 in src/app.ts",
            "serves trpc list @8 in src/app.ts",
            "serves trpc remove @9 in src/app.ts",
        ]
    );
    let sites = unread(&json, "src/app.ts");
    let lines: Vec<&str> = sites
        .iter()
        .map(|line| line.split(' ').next().unwrap_or_default())
        .collect();
    assert_eq!(lines, ["@10", "@11"], "{sites:?}");
    Ok(())
}
