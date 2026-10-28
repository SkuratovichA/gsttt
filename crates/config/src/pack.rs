//! Rule packs written in Lua: a chunk that returns one table, read into a [`Pack`].
//!
//! Built-in packs live next to their connector (`crates/lang-ts/packs/express.lua`) and reach this module as `(file name, source)` pairs; a user's packs arrive through `g.pack` in the config. Both go through the same sandbox as the config, so a built-in pack is also an example of a pack a user can write.

use gsttt_core::rules::Pack;
use mlua::Value;

use crate::sandbox::Sandbox;
use crate::value::to_json;

/// Why a pack does not load. The message names the pack and what Lua or the schema said.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("pack `{name}`: {message}")]
pub struct PackError {
    pub name: String,
    pub message: String,
}

impl PackError {
    fn new(name: &str, error: &impl std::fmt::Display) -> Self {
        Self {
            name: name.to_owned(),
            message: error.to_string(),
        }
    }
}

/// Evaluates `source`, named `name` in errors, and reads the table it returns as a [`Pack`].
pub fn eval_pack(name: &str, source: &str) -> Result<Pack, PackError> {
    let sandbox = Sandbox::new().map_err(|error| PackError::new(name, &error))?;
    eval_in(&sandbox, name, source)
}

/// Evaluates the built-in packs of a connector, `(file name, source)` pairs, in one Lua state.
pub fn builtin_packs(sources: &[(&str, &str)]) -> Result<Vec<Pack>, PackError> {
    let sandbox = Sandbox::new().map_err(|error| PackError::new("<builtin>", &error))?;
    sources
        .iter()
        .map(|(name, source)| eval_in(&sandbox, name, source))
        .collect()
}

fn eval_in(sandbox: &Sandbox, name: &str, source: &str) -> Result<Pack, PackError> {
    sandbox.reset_budget();
    let value: Value = sandbox
        .load(name, source)
        .eval()
        .map_err(|error| PackError::new(name, &sandbox.explain(&error)))?;
    // The converter of the gsttt module, so a pack in a file reads exactly as one passed to `g.pack`: an empty table is an empty list.
    let json = to_json(&value, "the pack").map_err(|error| PackError::new(name, &error))?;
    serde_json::from_value(json).map_err(|error| PackError::new(name, &error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_is_a_returned_table() {
        let pack = eval_pack(
            "axios.lua",
            r#"
            return {
              name = "axios",
              language = "ts",
              detect = { "axios" },
              calls = {
                { callee = { "axios.*", "api.*" }, kind = "http", role = "calls", key = "$arg1", method = "$callee" },
              },
              chains = {
                { root = "prisma", kind = "table", role = "calls", segments = 1, case = "upper-first" },
              },
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            (pack.name.as_str(), pack.calls.len(), pack.chains.len()),
            ("axios", 1, 1)
        );
        assert_eq!(pack.embeds, []);
    }

    #[test]
    fn errors_name_the_pack_and_the_sandbox_holds() {
        let typo = eval_pack(
            "x.lua",
            r#"return { name = "x", language = "ts", call = {} }"#,
        )
        .unwrap_err();
        assert!(
            typo.to_string().starts_with("pack `x.lua`: ") && typo.message.contains("call"),
            "{typo}"
        );
        let escape = eval_pack("evil.lua", r#"os.execute("true") return {}"#).unwrap_err();
        assert!(escape.message.contains("os"), "{escape}");
        let all = builtin_packs(&[
            ("a.lua", r#"return { name = "a", language = "ts" }"#),
            ("b.lua", r#"return { name = "b", language = "python" }"#),
        ])
        .unwrap();
        assert_eq!(
            all.iter()
                .map(|pack| pack.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
}
