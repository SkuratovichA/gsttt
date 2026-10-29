//! The tools the server lists.

use gsttt_core::query::LIST_CAP;
use gsttt_core::testing::json::descriptions;
use gsttt_core::text::thousands;
use gsttt_mcp::BUDGET;
use gsttt_scenario::scenario;
use serde_json::json;

use crate::fake::Fake;
use crate::session::Session;

#[scenario("mcp", "tools/list names the seven tools")]
#[tokio::test]
async fn tools_list_names_the_seven_tools_each_with_a_description_and_a_schema() {
    let session = Session::start(Fake::small()).await;

    let tools = session.tools().await;

    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "impact",
            "dependents",
            "dependencies",
            "path",
            "contract",
            "coverage",
            "index_status"
        ]
    );
    for tool in &tools {
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            !description.is_empty(),
            "`{}` has no description",
            tool.name
        );
        assert_eq!(
            tool.input_schema.get("type"),
            Some(&json!("object")),
            "`{}` has no input schema",
            tool.name
        );
    }
}

#[scenario("mcp", "The description of impact speaks of names")]
#[tokio::test]
async fn the_description_of_impact_speaks_of_names_and_direct_callers() {
    let session = Session::start(Fake::small()).await;

    let tools = session.tools().await;

    let impact = tools
        .iter()
        .find(|tool| tool.name == "impact")
        .expect("impact is listed");
    let description = impact.description.as_deref().unwrap_or_default();
    assert!(
        description
            .contains("one function, class, method or variable is asked with its name in `names`"),
        "{description}"
    );
    assert!(description.contains("direct callers"), "{description}");
    assert!(
        description.contains("ask again about a caller to go further"),
        "{description}"
    );
    assert!(
        impact
            .input_schema
            .get("properties")
            .and_then(|properties| properties.get("names"))
            .is_some(),
        "the schema of impact offers `names`"
    );
}

#[tokio::test]
async fn a_description_reaches_the_model_as_one_line() {
    let session = Session::start(Fake::small()).await;

    let tools = session.tools().await;

    let listing = serde_json::to_value(&tools).unwrap();
    let all = descriptions(&listing);
    assert!(
        all.len() > 7,
        "the walk finds the tools' and the arguments' descriptions"
    );
    for description in all {
        assert!(!description.contains('\n'), "{description:?}");
    }
}

#[tokio::test]
async fn every_all_argument_says_where_an_answer_stops() {
    let session = Session::start(Fake::small()).await;

    let tools = session.tools().await;

    let descriptions: Vec<&str> = tools
        .iter()
        .filter_map(|tool| {
            let argument = tool.input_schema.get("properties")?.get("all")?;
            argument.get("description")?.as_str()
        })
        .collect();
    assert_eq!(
        descriptions.len(),
        6,
        "every tool but `index_status` takes `all`"
    );
    for description in descriptions {
        assert!(description.contains(&thousands(BUDGET)), "{description}");
        assert!(description.contains(&LIST_CAP.to_string()), "{description}");
    }
}

#[tokio::test]
async fn the_whole_listing_is_read_before_it_changes() {
    let session = Session::start(Fake::small()).await;

    let tools = session.tools().await;

    // The descriptions and the schemas are what the model reads to choose and to fill in a call.
    insta::assert_json_snapshot!(tools);
}
