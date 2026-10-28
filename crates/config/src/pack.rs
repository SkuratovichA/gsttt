//! Rule packs written in TOML: a file with the fields of a [`Pack`], read with the same limits as the config.
//!
//! Built-in packs live next to their connector (`crates/lang-ts/packs/express.toml`) and reach this module as `(name, source)` pairs; a user's packs are the files of `.gsttt/packs/`. Both are read the same way, so a built-in pack is also an example of a pack a user can write.

use gsttt_core::rules::Pack;

use crate::document::{Located, parse, read_typed};

/// Why a pack does not load.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PackError {
    /// The file the pack is read from, as errors show it: `.gsttt/packs/ktor.toml`, or `axios.toml` for a built-in pack.
    pub file: String,
    /// `<file>:<line>:<column>: <message>`, which names the file, the place in it and what was wrong there.
    pub message: String,
}

/// Reads the pack in `text`, which is the content of `file`. A pack is named after its file, so that the file that defines a pack is the one a `disable_packs` entry or an error points at.
pub fn read_pack(file: &str, text: &str) -> Result<Pack, PackError> {
    read(file, text).map_err(|error| PackError {
        file: file.to_owned(),
        message: error.render(file, text),
    })
}

/// Reads the built-in packs of a connector, `(name, TOML source)` pairs, each as the file `<name>.toml`.
pub fn builtin_packs(sources: &[(&str, &str)]) -> Result<Vec<Pack>, PackError> {
    sources
        .iter()
        .map(|(name, text)| read_pack(&format!("{name}.toml"), text))
        .collect()
}

fn read(file: &str, text: &str) -> Result<Pack, Located> {
    let document = parse(text)?;
    let named_at = document.get_ref().iter().find_map(|(key, value)| {
        let key: &str = key.get_ref();
        (key == "name").then(|| value.span())
    });
    let pack: Pack = read_typed(document)?;
    let stem = file.rsplit('/').next().unwrap_or(file);
    let stem = stem.strip_suffix(".toml").unwrap_or(stem);
    match named_at {
        Some(span) if pack.name != stem => Err(Located::new(
            format!(
                "`name` is `{}`, but a pack is named after its file: it must be `{stem}`",
                pack.name
            ),
            span,
        )),
        _ => Ok(pack),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_is_a_table_of_fields() {
        let pack = read_pack(
            "axios.toml",
            r#"
name = "axios"
language = "ts"
detect = ["axios"]

[[calls]]
callee = ["axios.*", "api.*"]
kind = "http"
role = "calls"
key = "$arg1"
method = "$callee"

[[chains]]
root = "prisma"
kind = "table"
role = "calls"
segments = 1
case = "upper-first"
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
    fn built_in_packs_are_read_in_the_order_given_and_named_after_their_files() {
        let packs = builtin_packs(&[
            ("a", "name = \"a\"\nlanguage = \"ts\"\n"),
            ("b", "name = \"b\"\nlanguage = \"python\"\n"),
        ])
        .unwrap();
        let names: Vec<&str> = packs.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);

        let wrong = builtin_packs(&[("c", "name = \"d\"\nlanguage = \"ts\"\n")]).unwrap_err();
        assert!(
            wrong.message.starts_with("c.toml:1:8: ") && wrong.message.contains("must be `c`"),
            "{wrong}"
        );
        assert_eq!(wrong.file, "c.toml");
    }

    #[test]
    fn a_pack_without_a_name_is_refused_by_the_type_not_by_the_file_name() {
        let error = read_pack("x.toml", "language = \"ts\"\n").unwrap_err();
        assert!(error.message.contains("missing field `name`"), "{error}");
    }
}
