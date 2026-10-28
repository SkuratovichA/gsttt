//! Prisma schemas.

use gsttt_contracts::PRISMA;
use gsttt_core::fact::ParseStatus;

use crate::support::{Run, module};

#[gsttt_scenario::scenario("contract-formats", "A model declares its name and its table")]
#[test]
fn a_model_declares_its_name_and_its_table() {
    let run = Run::new("prisma_model", &[module(&PRISMA)]);
    assert_eq!(run.status("prisma/schema.prisma"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("prisma/schema.prisma"),
        vec![
            "calls env DATABASE_URL @1",
            "declares table booking_items @3",
            "declares table bookingitem @3",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "A brace inside a string does not end a model")]
#[test]
fn a_brace_inside_a_string_does_not_end_a_model() {
    let run = Run::new("prisma_strings", &[module(&PRISMA)]);
    assert_eq!(run.status("prisma/schema.prisma"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("prisma/schema.prisma"),
        vec![
            "calls env DATABASE_URL @3",
            "declares table booking @6",
            "declares table bookings @10",
            "declares table cart @13",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}
