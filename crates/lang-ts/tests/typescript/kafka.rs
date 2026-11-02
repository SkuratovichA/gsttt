//! The `kafka` pack: a producer is named by the type its receiver is declared with, because `emit` alone also matches event emitters.

use std::error::Error;

use gsttt_scenario::scenario;

use crate::support::{NOTHING, contracts, run, run_with_pack, unread};

#[test]
#[scenario("typescript", "A message pattern serves its topic")]
fn a_message_pattern_serves_its_topic() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(
        contracts(&json, "src/consumer.ts"),
        ["serves queue user.created @4", "serves queue user.get @7"]
    );
    assert_eq!(unread(&json, "src/consumer.ts"), NOTHING);
    insta::assert_json_snapshot!(json);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A client held in a constructor parameter property calls its topic"
)]
fn a_client_held_in_a_constructor_parameter_property_calls_its_topic() -> Result<(), Box<dyn Error>>
{
    let json = run("kafka")?;
    assert_eq!(
        contracts(&json, "src/producer-property.ts"),
        ["calls queue user.created @7"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A client held in a class property calls its topic")]
fn a_client_held_in_a_class_property_calls_its_topic() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(
        contracts(&json, "src/producer-field.ts"),
        ["calls queue user.deleted @8"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A producer service calls its topic")]
fn a_producer_service_calls_its_topic() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(
        contracts(&json, "src/producer-service.ts"),
        ["calls queue t @5"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "A producer received as a parameter calls its topic")]
fn a_producer_received_as_a_parameter_calls_its_topic() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(
        contracts(&json, "src/producer-param.ts"),
        ["calls queue t @4"]
    );
    Ok(())
}

#[test]
#[scenario("typescript", "An event emitter is no producer")]
fn an_event_emitter_is_no_producer() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(contracts(&json, "src/emitter.ts"), NOTHING);
    assert_eq!(unread(&json, "src/emitter.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A name bound to two kinds in one file is no producer")]
fn a_name_bound_to_two_kinds_in_one_file_is_no_producer() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(contracts(&json, "src/two-kinds.ts"), NOTHING);
    assert_eq!(unread(&json, "src/two-kinds.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario(
    "typescript",
    "A property of two types in two classes of one file is no producer"
)]
fn a_property_of_two_types_in_two_classes_of_one_file_is_no_producer() -> Result<(), Box<dyn Error>>
{
    let json = run("kafka")?;
    assert_eq!(contracts(&json, "src/two-classes.ts"), NOTHING);
    assert_eq!(unread(&json, "src/two-classes.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A consumer subscribed to a topic serves it")]
fn a_consumer_subscribed_to_a_topic_serves_it() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(
        contracts(&json, "src/consumer-subscribe.ts"),
        ["serves queue t @3"]
    );
    Ok(())
}

#[test]
fn a_producer_held_in_a_name_with_no_declared_type_is_not_read() -> Result<(), Box<dyn Error>> {
    let json = run("kafka")?;
    assert_eq!(contracts(&json, "src/untyped.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A project without kafkajs has no Kafka rules")]
fn a_project_without_kafkajs_has_no_kafka_rules() -> Result<(), Box<dyn Error>> {
    let json = run("kafka-absent")?;
    assert_eq!(contracts(&json, "src/consumer.ts"), NOTHING);
    assert_eq!(unread(&json, "src/consumer.ts"), NOTHING);
    Ok(())
}

#[test]
#[scenario("typescript", "A call rule reads only a receiver of a listed type")]
fn a_call_rule_reads_only_a_receiver_of_a_listed_type() -> Result<(), Box<dyn Error>> {
    let json = run_with_pack("kafka", PACK)?;
    // `Notifier` is typed, and the emitter beside it is not.
    assert_eq!(
        contracts(&json, "src/emitter.ts"),
        ["calls queue change @7"]
    );
    assert_eq!(
        contracts(&json, "src/producer-property.ts"),
        ["calls queue user.created @7"]
    );
    Ok(())
}

/// A project pack that lists `EventEmitter2` as the receiver type of `emit`.
const PACK: &str = r#"
name = "fixture"
language = "ts"

[[calls]]
callee = "*.*.emit"
receiver_types = ["EventEmitter2"]
kind = "queue"
role = "calls"
key = "$arg1"
"#;
