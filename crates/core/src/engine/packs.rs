//! Which rule packs a connector receives.

use crate::rules::Pack;
use crate::setup::Setup;

/// The packs for the connector named `language`: the built-in packs of its language except those the setup disables, where a pack of the setup replaces the built-in pack of the same name and otherwise is added.
pub fn merge_packs(builtin: &[Pack], setup: &Setup, language: &str) -> Vec<Pack> {
    let mut merged: Vec<Pack> = builtin
        .iter()
        .filter(|pack| pack.language == language && !setup.disable_packs.contains(&pack.name))
        .cloned()
        .collect();
    for user in setup.packs.iter().filter(|pack| pack.language == language) {
        match merged.iter_mut().find(|pack| pack.name == user.name) {
            Some(slot) => *slot = user.clone(),
            None => merged.push(user.clone()),
        }
    }
    merged
}
