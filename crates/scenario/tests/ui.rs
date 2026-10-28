//! The compile errors of `#[scenario]` as the compiler prints them for the attribute itself.
//!
//! `gsttt-spec` tests the expansion on token streams. Only a real compilation shows that the
//! attribute hands its arguments to that expansion and that the error lands on them. The cases sit
//! under `fixtures` so that the spec gate does not take their attributes for proofs.

#[gsttt_scenario::scenario("spec-gate", "Wrong arguments do not compile")]
#[test]
fn wrong_arguments_do_not_compile() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/fixtures/ui/*.rs");
}
