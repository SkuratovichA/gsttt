//! The connectors the binary ships, in the order the engine offers a file to them, and their rule packs.

use gsttt_config::{PackError, builtin_packs};
use gsttt_core::DynConnector;
use gsttt_core::rules::Pack;

/// A connector and the sources of the rule packs written for it, as `(name, TOML source)` pairs.
struct Builtin {
    connector: Box<dyn DynConnector>,
    packs: &'static [(&'static str, &'static str)],
}

/// The built-in connectors in registry order: `ts`, `python`, `rust`, `openapi`, `graphql`, `proto`, `prisma`,
/// `sql`, `env`. The order is behaviour, because the engine gives a file to the first connector that claims it.
///
/// A connector joins the list at its place in that order when its crate has one. Only `ts` has at this base.
fn builtins() -> Vec<Builtin> {
    vec![Builtin {
        connector: Box::new(gsttt_lang_ts::TYPESCRIPT),
        packs: gsttt_lang_ts::PACKS,
    }]
}

/// The connectors of a run, in the order the engine offers a file to them, and the built-in rule packs they read.
#[derive(Debug)]
pub struct Registry {
    connectors: Vec<Box<dyn DynConnector>>,
    packs: Vec<Pack>,
}

impl Registry {
    /// The built-in connectors, with every built-in pack read once.
    ///
    /// # Errors
    ///
    /// When a pack that ships with gsttt does not read, which is a bug in that pack.
    pub fn builtin() -> Result<Self, PackError> {
        let (connectors, sources): (Vec<_>, Vec<_>) = builtins()
            .into_iter()
            .map(|Builtin { connector, packs }| (connector, packs))
            .unzip();
        let sources: Vec<(&str, &str)> = sources.into_iter().flatten().copied().collect();
        Ok(Self {
            connectors,
            packs: builtin_packs(&sources)?,
        })
    }

    /// The connectors in registry order, in the form the engine takes.
    pub fn connectors(&self) -> Vec<&dyn DynConnector> {
        self.connectors.iter().map(AsRef::as_ref).collect()
    }

    /// The built-in rule packs of every connector. The engine hands each connector the ones written for it.
    pub fn packs(&self) -> &[Pack] {
        &self.packs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order of the design, which a connector's place in the registry has to follow.
    const ORDER: [&str; 9] = [
        "ts", "python", "rust", "openapi", "graphql", "proto", "prisma", "sql", "env",
    ];

    fn names(registry: &Registry) -> Vec<&'static str> {
        registry
            .connectors()
            .iter()
            .map(|connector| connector.meta().name)
            .collect()
    }

    /// A connector joins the registry at its place in the design's order, whichever of them are there.
    #[test]
    fn the_connectors_come_in_the_order_of_the_design() {
        let registry = Registry::builtin().unwrap();
        let registered = names(&registry);
        let in_design_order: Vec<&str> = ORDER
            .into_iter()
            .filter(|name| registered.contains(name))
            .collect();
        assert_eq!(registered, in_design_order);
    }

    #[test]
    fn typescript_is_registered_first() {
        let registry = Registry::builtin().unwrap();
        assert_eq!(names(&registry).first(), Some(&"ts"));
    }

    #[test]
    fn the_packs_of_every_connector_reach_the_registry() {
        let registry = Registry::builtin().unwrap();
        let shipped: usize = builtins().iter().map(|builtin| builtin.packs.len()).sum();
        assert_eq!(registry.packs().len(), shipped);
        assert!(registry.packs().iter().any(|pack| pack.language == "ts"));
    }
}
