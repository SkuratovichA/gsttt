//! GraphQL resolvers, resolver maps and documents embedded in templates.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{contracts, embedded, run};

#[test]
#[scenario("typescript", "A resolver method serves a root field")]
fn a_resolver_method_serves_a_root_field() -> Result<(), Box<dyn Error>> {
    let json = run("resolver-method")?;
    assert_eq!(
        contracts(&json, "src/bookings.resolver.ts"),
        [
            "serves graphql Query.bookings @7",
            "serves graphql Mutation.cancelBooking @10"
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A resolver map serves its fields")]
fn a_resolver_map_serves_its_fields() -> Result<(), Box<dyn Error>> {
    let json = run("resolver-map")?;
    assert_eq!(
        contracts(&json, "src/resolvers.ts"),
        [
            "serves graphql Query.bookings @6",
            "serves graphql Mutation.cancel @9"
        ]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario("typescript", "A tagged template is an embedded document")]
fn a_tagged_template_is_an_embedded_document() -> Result<(), Box<dyn Error>> {
    let json = run("tagged-template")?;
    assert_eq!(
        embedded(&json, "src/queries.ts"),
        ["graphql @5 query Bookings { bookings { id } }"]
    );
    insta::assert_json_snapshot!(json);
    Ok(())
}
