//! Which packs a project runs, on the `packs` fixture.

use gsttt_scenario::scenario;
use serde_json::json;

use crate::support::index;

#[scenario("rust", "Packs follow the manifest")]
#[test]
fn packs_follow_the_manifest() {
    let out = index("packs");
    assert_eq!(
        out.contracts("crates/api/src/main.rs"),
        ["7 serves http GET /bookings"]
    );
    // The same call in a package without axum is an ordinary method call: no site, and not an unread one either.
    let cli = out.sites("crates/cli/src/main.rs");
    assert_eq!(cli["contracts"], json!([]));
    assert_eq!(cli["unresolved"], json!([]));
}

#[test]
fn a_pack_follows_the_workspace_a_dev_dependency_and_a_renamed_package() {
    let out = index("packs");
    assert_eq!(
        out.contracts("crates/admin/src/main.rs"),
        ["7 serves http GET /bookings"]
    );
    assert_eq!(
        out.contracts("crates/tools/src/main.rs"),
        ["7 serves http GET /bookings"]
    );
}
