//! Protobuf services, HTTP bindings and the roots imports resolve from.

use gsttt_contracts::PROTO;
use gsttt_core::fact::ParseStatus;

use crate::support::{Run, module};

fn pair(spec: &str, target: &str) -> (String, String) {
    (spec.to_owned(), target.to_owned())
}

#[gsttt_scenario::scenario("contract-formats", "Services and HTTP bindings are declared")]
#[test]
fn services_and_http_bindings_are_declared() {
    let run = Run::new("proto_services", &[module(&PROTO)]);
    assert_eq!(run.status("booking.proto"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("booking.proto"),
        vec![
            "declares grpc booking.v1.BookingService/GetBooking @6",
            "declares http GET /v1/bookings/{} @7",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario(
    "contract-formats",
    "A path variable is a parameter wherever it stands"
)]
#[test]
fn a_path_variable_is_a_parameter_wherever_it_stands() {
    let run = Run::new("proto_templates", &[module(&PROTO)]);
    assert_eq!(run.status("things.proto"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("things.proto"),
        vec![
            "declares grpc things.v1.ThingService/CancelThing @6",
            "declares http POST /v1/things/{} @7",
            "declares grpc things.v1.ThingService/ListThings @9",
            "declares http GET /v1/{}/things @10",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "A binding set one field at a time is declared")]
#[test]
fn a_binding_set_one_field_at_a_time_is_declared() {
    let run = Run::new("proto_field_options", &[module(&PROTO)]);
    assert_eq!(run.status("things.proto"), &ParseStatus::Ok);
    assert_eq!(
        run.sites("things.proto"),
        vec![
            "declares grpc things.v1.ThingService/GetThing @6",
            "declares http GET /v1/things/{} @7",
            "declares grpc things.v1.ThingService/DropThing @9",
            "declares http DELETE /v1/things/{} @10",
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[gsttt_scenario::scenario("contract-formats", "Imports resolve from the proto root")]
#[test]
fn imports_resolve_from_the_proto_root() {
    let run = Run::new("proto_imports", &[module(&PROTO)]);
    assert_eq!(
        run.imports("proto/booking/v1/booking.proto"),
        vec![
            pair("booking/v1/types.proto", "proto/booking/v1/types.proto"),
            pair("google/protobuf/timestamp.proto", "builtin"),
            pair("google/api/annotations.proto", "unknown package"),
        ]
    );
    insta::assert_json_snapshot!(run.snapshot());
}

#[test]
fn buf_module_roots_and_workspace_directories_come_first() {
    let run = Run::new("proto_roots", &[module(&PROTO)]);
    assert_eq!(
        run.imports("services/shop/shop.proto"),
        vec![
            pair("common/v1/ids.proto", "libs/protos/common/v1/ids.proto"),
            pair("legacy/v1/old.proto", "libs/legacy/legacy/v1/old.proto"),
            pair("missing/v1/none.proto", "unknown package"),
        ]
    );
}
