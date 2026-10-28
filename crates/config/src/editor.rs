//! Editor support: where the JSON Schemas of the config and of a pack are written, and the starter `config.toml`. `gsttt init` writes all three.
//!
//! An editor with a TOML language server, taplo or Even Better TOML, checks a file against the schema named on its first line, `#:schema <path>`, with the path relative to the file. So the starter points at the schema with a path from `.gsttt/`, which keeps the setting valid wherever the repository is cloned and needs no editor settings of its own.

use crate::evaluate::PACKS_PATH;

/// Where `gsttt init` writes the JSON Schema of the config, relative to the repository root.
pub const CONFIG_SCHEMA_PATH: &str = ".gsttt/schema/config.json";

/// Where `gsttt init` writes the JSON Schema of a pack, relative to the repository root.
pub const PACK_SCHEMA_PATH: &str = ".gsttt/schema/pack.json";

/// The folder of the config, which the paths of the `#:schema` lines are relative to.
const WORKSPACE: &str = ".gsttt";

/// `path`, which is under [`WORKSPACE`] and given from the repository root, as a file inside the workspace writes it: from the workspace.
fn in_workspace(path: &str) -> &str {
    path.strip_prefix(WORKSPACE)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(path)
}

/// The first `.gsttt/config.toml`: the `#:schema` line, a note on packs, and the `detected` modules as commented tables to uncomment when one needs options. Detected modules run with their defaults without any line, so the starter changes nothing.
#[must_use]
pub fn starter(detected: &[&str]) -> String {
    let mut names = detected.to_vec();
    names.sort_unstable();
    names.dedup();

    let mut lines = vec![
        format!("#:schema ./{}", in_workspace(CONFIG_SCHEMA_PATH)),
        String::new(),
        format!(
            "# The editor checks this file against the schema named above. Packs go in {}/<name>.toml, each starting with `#:schema ../{}`.",
            in_workspace(PACKS_PATH),
            in_workspace(PACK_SCHEMA_PATH)
        ),
        String::new(),
    ];
    if names.is_empty() {
        lines.push("# No module was detected in this repository. Turn one on with a table, like the one below.".to_owned());
        lines.push("# [ts]".to_owned());
    } else {
        lines.push("# Detected in this repository. Each runs with its defaults; uncomment a table to give it options.".to_owned());
        lines.extend(names.iter().map(|name| format!("# [{name}]")));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}
