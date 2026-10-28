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
