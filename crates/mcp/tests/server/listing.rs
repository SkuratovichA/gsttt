//! The tools the server lists.

use gsttt_scenario::scenario;
use serde_json::{Value, json};

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

/// Every text under `value` that is a description.
fn descriptions(value: &Value) -> Vec<&str> {
    match value {
        Value::Object(members) => members
            .iter()
            .flat_map(|(name, member)| match member {
                Value::String(text) if name == "description" => vec![text.as_str()],
                _ => descriptions(member),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(descriptions).collect(),
        _ => Vec::new(),
    }
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
async fn the_whole_listing_is_read_before_it_changes() {
    let session = Session::start(Fake::small()).await;

    let tools = session.tools().await;

    // The descriptions and the schemas are what the model reads to choose and to fill in a call.
    insta::assert_json_snapshot!(tools);
}
