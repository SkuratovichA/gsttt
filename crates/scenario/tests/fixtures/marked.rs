// Included by `tests/attribute.rs`. It sits under `fixtures` so that the spec gate does not take
// this attribute for the proof of a scenario.

#[gsttt_scenario::scenario("queries", "Impact goes through wormholes")]
fn marked(left: u32, right: u32) -> u32 {
    left + right
}
