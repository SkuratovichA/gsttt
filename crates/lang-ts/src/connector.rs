//! The connector itself: what it is called, which files make it apply and how it opens projects.

use gsttt_core::RepoView;
use gsttt_core::connector::{Connector, ConnectorError, Erased, Meta};
use gsttt_core::rules::Pack;

use crate::options::TypeScriptOptions;
use crate::project::TsProject;
use crate::source::{CODE_EXTENSIONS, is_code};

/// The TypeScript and JavaScript connector.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeScript;

/// The connector in the form the engine takes.
pub static TYPESCRIPT: Erased<TypeScript> = Erased(TypeScript);

/// The rule packs shipped with the connector, as `(name, TOML source)` pairs. The name is the file name without `.toml`, which is also the `name` the pack declares.
pub const PACKS: &[(&str, &str)] = &[
    ("axios", include_str!("../packs/axios.toml")),
    ("drizzle", include_str!("../packs/drizzle.toml")),
    ("env", include_str!("../packs/env.toml")),
    ("express", include_str!("../packs/express.toml")),
    ("fetch", include_str!("../packs/fetch.toml")),
    ("graphql", include_str!("../packs/graphql.toml")),
    ("nestjs", include_str!("../packs/nestjs.toml")),
    ("prisma", include_str!("../packs/prisma.toml")),
    ("trpc", include_str!("../packs/trpc.toml")),
    ("typeorm", include_str!("../packs/typeorm.toml")),
];

impl Connector for TypeScript {
    type Options = TypeScriptOptions;
    type Project = TsProject;

    fn meta(&self) -> Meta {
        Meta {
            name: "ts",
            version: 3,
            summary: "TypeScript and JavaScript: imports, tsconfig paths, workspaces and the contracts of web frameworks",
            extensions: CODE_EXTENSIONS,
        }
    }

    fn detect(&self, repo: &RepoView) -> bool {
        repo.files().any(|(_, path)| is_code(path.as_str()))
    }

    fn open(
        &self,
        repo: &RepoView,
        options: &TypeScriptOptions,
        packs: &[Pack],
    ) -> Result<Vec<TsProject>, ConnectorError> {
        Ok(TsProject::open_all(repo, options, packs))
    }
}
