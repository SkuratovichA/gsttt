//! The salt of a project over the inputs it is made of.

use gsttt_config::read_pack;
use gsttt_core::rules::Pack;
use ra_ap_edition::Edition;

use super::salt_for;

fn pack(name: &str) -> Pack {
    let source = format!(
        r#"name = "{name}"
language = "rust"

[[calls]]
callee = "env.var"
kind = "env"
role = "calls"
key = "$arg1"
"#
    );
    read_pack(&format!("{name}.toml"), &source).unwrap()
}

#[test]
fn the_salt_follows_the_edition_and_the_packs() {
    let salt = salt_for(Edition::Edition2021, &[pack("a")]);
    assert_eq!(salt, salt_for(Edition::Edition2021, &[pack("a")]));
    assert_ne!(salt, salt_for(Edition::Edition2018, &[pack("a")]));
    assert_ne!(salt, salt_for(Edition::Edition2021, &[pack("b")]));
    assert_ne!(salt, salt_for(Edition::Edition2021, &[]));
}
