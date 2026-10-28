use gsttt_config::builtin_packs;

#[gsttt_scenario::scenario("config", "Built-in packs share the sandbox")]
#[test]
fn a_built_in_pack_cannot_load_code() {
    let error = builtin_packs(&[
        ("fine.lua", r#"return { name = "fine", language = "ts" }"#),
        ("loader.lua", r#"loadfile("x.lua") return {}"#),
    ])
    .unwrap_err();
    assert_eq!(error.name, "loader.lua");
    assert!(
        error
            .to_string()
            .starts_with("pack `loader.lua`: loader.lua:1: "),
        "{error}"
    );
}
