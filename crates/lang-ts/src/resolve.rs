//! Import resolution over the repository's own file list; the disk is never touched.

use gsttt_core::fact::{Import, ImportKind, Resolution, UnresolvedReason};
use gsttt_core::{FileId, RepoPath};

use crate::builtins::is_builtin;
use crate::config::Effective;
use crate::json::Json;
use crate::pathing::{
    ancestors, is_config_name, is_relative, join_dir, join_file, split_package, strip_dir,
};
use crate::shared::Shared;

/// What is appended to a specifier that names no file, in the order TypeScript tries it.
const EXTENSIONS: [&str; 9] = [
    ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs",
];

/// An extension a build writes, and the extensions of the sources it may have been built from.
const BUILT: [(&str, &[&str]); 7] = [
    (".d.ts", &[".ts", ".tsx"]),
    (".d.mts", &[".mts"]),
    (".d.cts", &[".cts"]),
    (".js", &[".ts", ".tsx", ".d.ts"]),
    (".jsx", &[".tsx"]),
    (".mjs", &[".mts", ".d.mts"]),
    (".cjs", &[".cts", ".d.cts"]),
];

/// Where `import` written in the file `from` of the project rooted at `root` leads.
pub(crate) fn resolve(shared: &Shared, root: &str, from: &RepoPath, import: &Import) -> Resolution {
    let spec = import.spec.as_str();
    let dir = from.parent();
    if matches!(import.kind, ImportKind::Reference) {
        return if is_config_name(from.file_name()) {
            config_reference(shared, root, from.as_str(), spec)
        } else {
            relative(shared, dir, spec)
        };
    }
    if spec.starts_with("node:") {
        return Resolution::Builtin;
    }
    if is_relative(spec) {
        return relative(shared, dir, spec);
    }
    if spec.starts_with('#') {
        return subpath_import(shared, dir, spec);
    }
    bare(shared, root, dir, spec)
}

fn not_found() -> Resolution {
    Resolution::Unresolved(UnresolvedReason::NotFound)
}

fn found(file: Option<FileId>) -> Resolution {
    file.map_or_else(not_found, |file| Resolution::File { file })
}

fn relative(shared: &Shared, dir: &str, spec: &str) -> Resolution {
    found(join_dir(dir, spec).and_then(|path| probe(shared, &path)))
}

/// The `extends` of a tsconfig: a file of the repository, a package that is declared, or neither.
fn config_reference(shared: &Shared, root: &str, from: &str, spec: &str) -> Resolution {
    if let Some(file) = shared
        .extends_target(from, spec)
        .and_then(|path| shared.repo.lookup(&path))
    {
        return Resolution::File { file };
    }
    if is_relative(spec) {
        return not_found();
    }
    match split_package(spec) {
        Some((name, _)) if declared(shared, root, name) => Resolution::External {
            package: name.to_owned(),
        },
        Some(_) => Resolution::Unresolved(UnresolvedReason::UnknownPackage),
        None => not_found(),
    }
}

/// A specifier that is not a path: an alias, a file under `baseUrl`, a builtin, or a package.
fn bare(shared: &Shared, root: &str, dir: &str, spec: &str) -> Resolution {
    if let Some(effective) = shared.config_for(dir) {
        let by_alias = alias(shared, effective, spec);
        let by_base = || {
            effective
                .base_url
                .as_deref()
                .and_then(|base| join_dir(base, spec))
                .and_then(|path| probe(shared, &path))
        };
        if let Some(file) = by_alias.or_else(by_base) {
            return Resolution::File { file };
        }
    }
    if is_builtin(spec) {
        return Resolution::Builtin;
    }
    let Some((name, subpath)) = split_package(spec) else {
        return not_found();
    };
    if let Some(package_dir) = shared.packages.get(name) {
        return workspace_entry(shared, package_dir, subpath).map_or_else(
            || Resolution::External {
                package: name.to_owned(),
            },
            |file| Resolution::File { file },
        );
    }
    if declared(shared, root, name) {
        Resolution::External {
            package: name.to_owned(),
        }
    } else {
        Resolution::Unresolved(UnresolvedReason::UnknownPackage)
    }
}

/// Whether the project's manifest, or the root one, declares the package (or its `@types` twin).
fn declared(shared: &Shared, root: &str, name: &str) -> bool {
    let types = types_name(name);
    [root, ""]
        .into_iter()
        .filter_map(|dir| shared.manifests.get(dir))
        .any(|manifest| {
            manifest.dependencies.contains(name) || manifest.dependencies.contains(&types)
        })
}

/// The name of the `DefinitelyTyped` package for `name`: `@types/react`, `@types/acme__ui`.
fn types_name(name: &str) -> String {
    match name
        .strip_prefix('@')
        .and_then(|scoped| scoped.split_once('/'))
    {
        Some((scope, package)) => format!("@types/{scope}__{package}"),
        None => format!("@types/{name}"),
    }
}

/// A `paths` alias of the governing config, using the pattern with the longest prefix.
fn alias(shared: &Shared, effective: &Effective, spec: &str) -> Option<FileId> {
    let mappings = effective.paths.as_ref()?;
    let (mapping, captured) = mappings
        .iter()
        .filter_map(|mapping| capture(&mapping.pattern, spec).map(|captured| (mapping, captured)))
        .max_by_key(|(mapping, _)| mapping.pattern.find('*').unwrap_or(usize::MAX))?;
    let base = effective.base_url.as_deref().unwrap_or(&mapping.dir);
    mapping.targets.iter().find_map(|target| {
        join_dir(base, &target.replacen('*', captured, 1)).and_then(|path| probe(shared, &path))
    })
}

/// What the `*` of `pattern` stands for in `spec`; the empty string for a pattern without one that is `spec` itself.
fn capture<'s>(pattern: &str, spec: &'s str) -> Option<&'s str> {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => spec.strip_prefix(prefix)?.strip_suffix(suffix),
        None => (pattern == spec).then_some(""),
    }
}

/// The source file a workspace package gives for `subpath` (`""` for the package itself).
fn workspace_entry(shared: &Shared, package_dir: &str, subpath: &str) -> Option<FileId> {
    let manifest = shared.manifests.get(package_dir)?;
    let mapped = manifest
        .exports
        .as_ref()
        .and_then(|exports| exports_target(exports, subpath, &shared.conditions))
        .and_then(|target| built_file(shared, package_dir, &target));
    if mapped.is_some() {
        return mapped;
    }
    if subpath.is_empty() {
        let entries = [&manifest.types, &manifest.module, &manifest.main];
        let by_field = entries
            .into_iter()
            .flatten()
            .find_map(|entry| built_file(shared, package_dir, entry));
        return by_field
            .or_else(|| probe(shared, &join_file(package_dir, "index")))
            .or_else(|| {
                probe(
                    shared,
                    &join_file(&source_root(shared, package_dir), "index"),
                )
            });
    }
    join_dir(package_dir, subpath)
        .and_then(|path| probe(shared, &path))
        .or_else(|| {
            join_dir(&source_root(shared, package_dir), subpath)
                .and_then(|path| probe(shared, &path))
        })
}

/// Where a package keeps its sources: the `rootDir` of its config, else `src`.
fn source_root(shared: &Shared, package_dir: &str) -> String {
    shared
        .config_for(package_dir)
        .and_then(|effective| effective.root_dir.clone())
        .unwrap_or_else(|| join_file(package_dir, "src"))
}

/// The source file behind `target`, a path a manifest names relative to the package: the file itself, or what the
/// package's build writes it from.
fn built_file(shared: &Shared, package_dir: &str, target: &str) -> Option<FileId> {
    let path = join_dir(package_dir, target)?;
    if let Some(file) = probe(shared, &path) {
        return Some(file);
    }
    let by_config = shared.config_for(package_dir).and_then(|effective| {
        let out_dir = effective.out_dir.as_deref()?;
        let rest = strip_dir(&path, out_dir)?;
        let root = effective.root_dir.as_deref().unwrap_or(package_dir);
        probe(shared, &join_dir(root, rest)?)
    });
    if by_config.is_some() {
        return by_config;
    }
    let rest = strip_dir(&path, package_dir)?;
    let (first, tail) = rest.split_once('/')?;
    if matches!(first, "dist" | "build" | "lib" | "out") {
        return probe(shared, &join_file(&join_file(package_dir, "src"), tail));
    }
    None
}

/// The file a path stands for: the path itself, with an extension added, as a directory with an index, or the
/// TypeScript source of a built `.js` file.
pub(crate) fn probe(shared: &Shared, path: &str) -> Option<FileId> {
    let at = |candidate: &str| shared.repo.lookup(candidate);
    BUILT
        .iter()
        .find_map(|(extension, sources)| {
            let stem = path.strip_suffix(extension)?;
            sources
                .iter()
                .find_map(|source| at(&format!("{stem}{source}")))
        })
        .or_else(|| at(path))
        .or_else(|| {
            EXTENSIONS
                .iter()
                .find_map(|extension| at(&format!("{path}{extension}")))
        })
        .or_else(|| {
            EXTENSIONS
                .iter()
                .find_map(|extension| at(&join_file(path, &format!("index{extension}"))))
        })
}

/// A `#name` import: looked up in the nearest manifest that has an `imports` map.
fn subpath_import(shared: &Shared, dir: &str, spec: &str) -> Resolution {
    let Some((manifest_dir, imports)) = ancestors(dir).find_map(|dir| {
        shared
            .manifests
            .get(dir)
            .and_then(|manifest| manifest.imports.as_ref())
            .map(|imports| (dir, imports))
    }) else {
        return not_found();
    };
    let Some(target) = pattern_target(imports, spec, &shared.conditions) else {
        return not_found();
    };
    if is_relative(&target) {
        return found(built_file(shared, manifest_dir, &target));
    }
    split_package(&target).map_or_else(not_found, |(name, _)| Resolution::External {
        package: name.to_owned(),
    })
}

/// The target of `subpath` in a package's `exports`: `.` for the package, `./name` below it.
fn exports_target(exports: &Json, subpath: &str, conditions: &[String]) -> Option<String> {
    let key = if subpath.is_empty() {
        ".".to_owned()
    } else {
        format!("./{subpath}")
    };
    if exports
        .entries()
        .iter()
        .any(|entry| entry.key.starts_with('.'))
    {
        pattern_target(exports, &key, conditions)
    } else if subpath.is_empty() {
        pick_target(exports, conditions).map(str::to_owned)
    } else {
        None
    }
}

/// The target of `key` in a map of subpaths: an exact key, else the pattern with the longest prefix.
fn pattern_target(map: &Json, key: &str, conditions: &[String]) -> Option<String> {
    let entries = map.entries();
    if let Some(entry) = entries.iter().find(|entry| entry.key == key) {
        return pick_target(&entry.value, conditions).map(str::to_owned);
    }
    let (entry, captured) = entries
        .iter()
        .filter_map(|entry| {
            let (prefix, suffix) = entry.key.split_once('*')?;
            let captured = key.strip_prefix(prefix)?.strip_suffix(suffix)?;
            Some((entry, captured))
        })
        .max_by_key(|(entry, _)| entry.key.find('*').unwrap_or_default())?;
    pick_target(&entry.value, conditions).map(|target| target.replacen('*', captured, 1))
}

/// The first target of a conditional value whose condition is active; a string is its own target.
fn pick_target<'j>(value: &'j Json, conditions: &[String]) -> Option<&'j str> {
    match value {
        Json::Str(target) => Some(target.as_str()),
        Json::Array(items) => items.iter().find_map(|item| pick_target(item, conditions)),
        Json::Object(entries) => entries.iter().find_map(|entry| {
            let active = entry.key == "default" || conditions.contains(&entry.key);
            if active {
                pick_target(&entry.value, conditions)
            } else {
                None
            }
        }),
        Json::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_star_captures_the_middle() {
        assert_eq!(capture("@db/*", "@db/client"), Some("client"));
        assert_eq!(capture("@db/*", "@dx/client"), None);
        assert_eq!(capture("exact", "exact"), Some(""));
        assert_eq!(capture("a*b", "ab"), Some(""));
        assert_eq!(capture("ab*ba", "aba"), None);
    }

    #[test]
    fn types_packages_are_named_after_their_scope() {
        assert_eq!(types_name("react"), "@types/react");
        assert_eq!(types_name("@acme/ui"), "@types/acme__ui");
    }

    proptest! {
        #[test]
        fn capturing_never_panics(pattern in "[a-z*/@]{0,10}", spec in "[a-z/@]{0,10}") {
            let _ = capture(&pattern, &spec);
        }
    }
}
