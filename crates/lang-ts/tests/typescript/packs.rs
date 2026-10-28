//! The built-in packs as the connector ships them. The snapshot holds every rule of every pack, so a change to the way packs are written cannot change what they say without a reviewer seeing it.

use std::error::Error;

use gsttt_config::builtin_packs;

#[test]
fn every_built_in_pack_says_what_it_said() -> Result<(), Box<dyn Error>> {
    let packs = builtin_packs(gsttt_lang_ts::PACKS)?;
    insta::assert_json_snapshot!(packs);
    Ok(())
}
