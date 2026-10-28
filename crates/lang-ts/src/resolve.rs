//! Import resolution over the repository's own file list. The disk is read only when the list has found nothing, to tell
//! a file that git ignores from a file that does not exist. The reason `generated` is never produced: a file a code
//! generator will write cannot be told from one that is missing.

use gsttt_core::fact::{Import, ImportKind, Resolution, UnresolvedReason};
use gsttt_core::{FileId, RepoPath};

use crate::builtins::is_builtin;
use crate::config::Effective;
use crate::json::Json;
use crate::manifest::Manifest;
use crate::pathing::{
    ancestors, is_config_name, is_json, is_relative, join_file, split_package, strip_dir,
};
use crate::shared::Shared;

/// What is appended to a specifier that names no file, in the order tsc tries it. `.mts` and `.cts` are not here: tsc
/// reaches them only through a specifier that says `.mjs` or `.cjs`.
const EXTENSIONLESS: [&str; 5] = [".ts", ".tsx", ".d.ts", ".js", ".jsx"];

/// An extension a build writes and the extensions of the files it may be written from, in the order tsc tries them. A
/// declaration file is a source already and has no row.
const BUILT: [(&str, &[&str]); 4] = [
    (".js", &[".ts", ".tsx", ".d.ts", ".js", ".jsx"]),
    (".jsx", &[".tsx", ".jsx"]),
    (".mjs", &[".mts", ".d.mts", ".mjs"]),
    (".cjs", &[".cts", ".d.cts", ".cjs"]),
];

/// Extensions of files that are sources themselves: nothing is swapped in or appended to them.
const SOURCES: [&str; 4] = [".ts", ".tsx", ".mts", ".cts"];

/// What a workspace package gives for an import of one of its subpaths.
enum Lookup {
    /// A file of the package.
    File(FileId),
    /// The `exports` map of the package leaves the subpath out or blocks it with `null`: nothing can be imported.
    Hidden,
    /// The package names nothing the repository holds for the subpath; it is still the package that was meant.
    Unmapped,
}

/// The conditions an `exports` or `imports` map is read with for one import: the configured ones, and `import` or
/// `require` by the form of the import, never both.
struct Conditions<'a> {
    configured: &'a [String],
    form: &'static str,
}

impl Conditions<'_> {
    fn holds(&self, condition: &str) -> bool {
        condition == self.form
            || self
                .configured
                .iter()
                .any(|configured| configured == condition)
    }
}

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
    let conditions = Conditions {
        configured: &shared.conditions,
        form: if matches!(import.kind, ImportKind::Require) {
            "require"
        } else {
            "import"
        },
    };
    if spec.starts_with('#') {
        return subpath_import(shared, dir, spec, &conditions);
    }
    bare(shared, root, from.as_str(), spec, &conditions)
}

fn not_found() -> Resolution {
    Resolution::Unresolved(UnresolvedReason::NotFound)
}

fn found(file: Option<FileId>) -> Resolution {
    file.map_or_else(not_found, |file| Resolution::File { file })
}

fn relative(shared: &Shared, dir: &str, spec: &str) -> Resolution {
    // A trailing slash, `.` and `..` name a directory, which only an index file can stand for.
    let directory = spec.ends_with('/') || matches!(spec.rsplit('/').next(), Some("." | ".."));
    let path = RepoPath::join_dir(dir, spec);
    located(shared, path.iter().map(String::as_str), directory)
}

/// Where the first of `paths` that names a file leads. A path the view lacks but the disk has is an ignored file, which
/// is not the same as a missing one.
fn located<'p>(
    shared: &Shared,
    paths: impl Iterator<Item = &'p str> + Clone,
    directory: bool,
) -> Resolution {
    if let Some(file) = paths
        .clone()
        .find_map(|path| locate(shared, path, directory))
    {
        return Resolution::File { file };
    }
    if paths
        .into_iter()
        .any(|path| ignored(shared, path, directory))
    {
        return Resolution::Unresolved(UnresolvedReason::Ignored);
    }
    not_found()
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
fn bare(
    shared: &Shared,
    root: &str,
    from: &str,
    spec: &str,
    conditions: &Conditions<'_>,
) -> Resolution {
    let mut leads = Vec::new();
    if let Some(effective) = shared.config_of(from) {
        leads = alias(effective, spec);
        leads.extend(
            effective
                .base_url
                .as_deref()
                .and_then(|base| RepoPath::join_dir(base, spec)),
        );
        if let Some(file) = leads.iter().find_map(|path| probe(shared, path)) {
            return Resolution::File { file };
        }
    }
    let resolved = package(shared, root, spec, conditions);
    // The disk is read only for an import that leads nowhere: a package or a builtin is never looked up there.
    if matches!(resolved, Resolution::Unresolved(_))
        && leads.iter().any(|path| ignored(shared, path, false))
    {
        return Resolution::Unresolved(UnresolvedReason::Ignored);
    }
    resolved
}

/// A specifier no alias took: a builtin, a workspace package, or an installed one.
fn package(shared: &Shared, root: &str, spec: &str, conditions: &Conditions<'_>) -> Resolution {
    if is_builtin(spec) {
        return Resolution::Builtin;
    }
    let Some((name, subpath)) = split_package(spec) else {
        return not_found();
    };
    if let Some(package_dir) = shared.packages.get(name) {
        return match workspace_entry(shared, package_dir, subpath, conditions) {
            Lookup::File(file) => Resolution::File { file },
            Lookup::Hidden => not_found(),
            Lookup::Unmapped => Resolution::External {
                package: name.to_owned(),
            },
        };
    }
    if declared(shared, root, name) {
        Resolution::External {
            package: name.to_owned(),
        }
    } else {
        Resolution::Unresolved(UnresolvedReason::UnknownPackage)
    }
}

/// Whether the manifest of the project, or of a project around it, declares the package (or its `@types` twin).
fn declared(shared: &Shared, root: &str, name: &str) -> bool {
    let types = types_name(name);
    shared.manifests_above(root).any(|manifest| {
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

/// The paths the `paths` aliases of the governing config give `spec`, for the pattern with the longest prefix.
fn alias(effective: &Effective, spec: &str) -> Vec<String> {
    let Some(mappings) = effective.paths.as_ref() else {
        return Vec::new();
    };
    let matching = mappings
        .iter()
        .filter_map(|mapping| capture(&mapping.pattern, spec).map(|captured| (mapping, captured)));
    let Some((mapping, captured)) =
        first_longest(matching, |(mapping, _)| prefix_len(&mapping.pattern))
    else {
        return Vec::new();
    };
    let base = effective.base_url.as_deref().unwrap_or(&mapping.dir);
    mapping
        .targets
        .iter()
        .filter_map(|target| RepoPath::join_dir(base, &target.replacen('*', captured, 1)))
        .collect()
}

/// The item whose prefix is the longest, and the first listed of equals: how tsc and Node pick among patterns.
fn first_longest<T>(items: impl Iterator<Item = T>, prefix: impl Fn(&T) -> usize) -> Option<T> {
    items
        .fold(None, |best: Option<(usize, T)>, item| {
            let length = prefix(&item);
            match best {
                Some((longest, _)) if longest >= length => best,
                _ => Some((length, item)),
            }
        })
        .map(|(_, item)| item)
}

/// How much of a pattern comes before its `*`; a pattern with none is an exact match and beats every other.
fn prefix_len(pattern: &str) -> usize {
    pattern.find('*').unwrap_or(usize::MAX)
}

/// What the `*` of `pattern` stands for in `spec`; the empty string for a pattern without one that is `spec` itself.
fn capture<'s>(pattern: &str, spec: &'s str) -> Option<&'s str> {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => spec.strip_prefix(prefix)?.strip_suffix(suffix),
        None => (pattern == spec).then_some(""),
    }
}

/// The source file a workspace package gives for `subpath` (`""` for the package itself).
fn workspace_entry(
    shared: &Shared,
    package_dir: &str,
    subpath: &str,
    conditions: &Conditions<'_>,
) -> Lookup {
    let Some(manifest) = shared.manifests.get(package_dir) else {
        return Lookup::Unmapped;
    };
    if let Some(exports) = &manifest.exports {
        // The map is the whole interface of the package: what it leaves out, or blocks with `null`, cannot be imported.
        let Some(target) = exports_target(exports, subpath, conditions) else {
            return Lookup::Hidden;
        };
        if let Some(file) = built_file(shared, package_dir, &target) {
            return Lookup::File(file);
        }
    }
    let by_path = if subpath.is_empty() {
        package_file(shared, package_dir, manifest)
    } else {
        subpath_file(shared, package_dir, subpath)
    };
    by_path.map_or(Lookup::Unmapped, Lookup::File)
}

/// The file a manifest without a usable `exports` map gives for the package itself.
fn package_file(shared: &Shared, package_dir: &str, manifest: &Manifest) -> Option<FileId> {
    let entries = [&manifest.types, &manifest.module, &manifest.main];
    entries
        .into_iter()
        .flatten()
        .find_map(|entry| built_file(shared, package_dir, entry))
        .or_else(|| probe(shared, &join_file(package_dir, "index")))
        .or_else(|| {
            probe(
                shared,
                &join_file(&source_root(shared, package_dir), "index"),
            )
        })
}

/// The file below a package that a path inside it names, in the package or in its source root.
fn subpath_file(shared: &Shared, package_dir: &str, subpath: &str) -> Option<FileId> {
    RepoPath::join_dir(package_dir, subpath)
        .and_then(|path| probe(shared, &path))
        .or_else(|| {
            RepoPath::join_dir(&source_root(shared, package_dir), subpath)
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
    let path = RepoPath::join_dir(package_dir, target)?;
    if let Some(file) = probe(shared, &path) {
        return Some(file);
    }
    let by_config = shared.config_for(package_dir).and_then(|effective| {
        let out_dir = effective.out_dir.as_deref()?;
        let rest = strip_dir(&path, out_dir)?;
        let root = effective.root_dir.as_deref().unwrap_or(package_dir);
        probe(shared, &RepoPath::join_dir(root, rest)?)
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

/// Calls `visit` with each path `path` may stand for, in the order tsc tries them, until it answers `true`. A path with
/// a built extension (`.js`) stands for the sources it may be built from. A source (`.ts`) stands for itself. Any other
/// name stands for itself, for the name with an extension added, and for the index file of a directory of that name.
/// `directory` says the specifier named a directory (it ended in `/`, `.` or `..`), so only the index files count. Every
/// candidate is built in one buffer.
fn candidates(path: &str, directory: bool, mut visit: impl FnMut(&str) -> bool) {
    let mut buffer = String::with_capacity(path.len() + 16);
    if !directory {
        let built = BUILT.iter().find_map(|(extension, sources)| {
            path.strip_suffix(extension).map(|stem| (stem, *sources))
        });
        if let Some((stem, sources)) = built {
            for source in sources {
                buffer.clear();
                buffer.push_str(stem);
                buffer.push_str(source);
                if visit(&buffer) {
                    return;
                }
            }
            return;
        }
        if visit(path) || SOURCES.iter().any(|extension| path.ends_with(extension)) {
            return;
        }
        for extension in EXTENSIONLESS {
            buffer.clear();
            buffer.push_str(path);
            buffer.push_str(extension);
            if visit(&buffer) {
                return;
            }
        }
    }
    for extension in EXTENSIONLESS {
        buffer.clear();
        buffer.push_str(path);
        if !path.is_empty() {
            buffer.push('/');
        }
        buffer.push_str("index");
        buffer.push_str(extension);
        if visit(&buffer) {
            return;
        }
    }
}

/// The file in the view that `path` stands for; see [`candidates`].
fn locate(shared: &Shared, path: &str, directory: bool) -> Option<FileId> {
    let mut found = None;
    candidates(path, directory, |candidate| {
        found = shared.repo.lookup(candidate);
        found.is_some()
    });
    found
}

/// Whether a file `path` stands for exists on disk although the view does not list it. It looks at the same candidates
/// in the same order as [`locate`].
fn ignored(shared: &Shared, path: &str, directory: bool) -> bool {
    // The path itself may be a directory on disk, which is no module: it counts only when it ends like a file.
    let file_like = is_json(path)
        || SOURCES.iter().any(|extension| path.ends_with(extension))
        || BUILT.iter().any(|(extension, _)| path.ends_with(extension));
    let mut hit = false;
    candidates(path, directory, |candidate| {
        hit = (file_like || candidate != path) && shared.repo.exists_on_disk(candidate);
        hit
    });
    hit
}

/// The file a path stands for: the path itself, with an extension added, as a directory with an index, or the source
/// of a built file.
pub(crate) fn probe(shared: &Shared, path: &str) -> Option<FileId> {
    locate(shared, path, false)
}

/// A `#name` import: looked up in the nearest manifest that has an `imports` map.
fn subpath_import(
    shared: &Shared,
    dir: &str,
    spec: &str,
    conditions: &Conditions<'_>,
) -> Resolution {
    let Some((manifest_dir, imports)) = ancestors(dir).find_map(|dir| {
        shared
            .manifests
            .get(dir)
            .and_then(|manifest| manifest.imports.as_ref())
            .map(|imports| (dir, imports))
    }) else {
        return not_found();
    };
    let Some(target) = pattern_target(imports, spec, conditions) else {
        return not_found();
    };
    if is_relative(&target) {
        return found(built_file(shared, manifest_dir, &target));
    }
    if is_builtin(&target) {
        return Resolution::Builtin;
    }
    split_package(&target).map_or_else(not_found, |(name, _)| Resolution::External {
        package: name.to_owned(),
    })
}

/// The target of `subpath` in a package's `exports`: `.` for the package, `./name` below it. `None` when the map leaves
/// the subpath out, blocks it with `null`, or has no active condition for it.
fn exports_target(exports: &Json, subpath: &str, conditions: &Conditions<'_>) -> Option<String> {
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
        pick_target(exports, conditions)
            .and_then(Picked::path)
            .map(str::to_owned)
    } else {
        None
    }
}

/// The target of `key` in a map of subpaths: an exact key, else the pattern with the longest prefix and the first listed
/// of equals.
fn pattern_target(map: &Json, key: &str, conditions: &Conditions<'_>) -> Option<String> {
    let entries = map.entries();
    if let Some(entry) = entries.iter().find(|entry| entry.key == key) {
        return pick_target(&entry.value, conditions)
            .and_then(Picked::path)
            .map(str::to_owned);
    }
    let matching = entries.iter().filter_map(|entry| {
        let (prefix, suffix) = entry.key.split_once('*')?;
        let captured = key.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some((entry, captured))
    });
    let (entry, captured) = first_longest(matching, |(entry, _)| prefix_len(&entry.key))?;
    pick_target(&entry.value, conditions)
        .and_then(Picked::path)
        .map(|target| target.replacen('*', captured, 1))
}

/// What a conditional target comes to for one import.
#[derive(Debug, PartialEq)]
enum Picked<'j> {
    /// A path inside the package.
    Path(&'j str),
    /// `null`: the subpath is closed to importers, whatever conditions follow.
    Blocked,
}

impl<'j> Picked<'j> {
    fn path(self) -> Option<&'j str> {
        match self {
            Self::Path(target) => Some(target),
            Self::Blocked => None,
        }
    }
}

/// The first target of a conditional value, in the order its keys are written, whose condition is active; a string is
/// its own target and `null` closes the path. An object with no active key leaves the choice to what follows it.
fn pick_target<'j>(value: &'j Json, conditions: &Conditions<'_>) -> Option<Picked<'j>> {
    match value {
        Json::Str(target) => Some(Picked::Path(target.as_str())),
        Json::Null => Some(Picked::Blocked),
        Json::Array(items) => items.iter().find_map(|item| pick_target(item, conditions)),
        Json::Object(entries) => entries.iter().find_map(|entry| {
            conditions
                .holds(&entry.key)
                .then(|| pick_target(&entry.value, conditions))
                .flatten()
        }),
        Json::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn tried(path: &str, directory: bool) -> Vec<String> {
        let mut all = Vec::new();
        candidates(path, directory, |candidate| {
            all.push(candidate.to_owned());
            false
        });
        all
    }

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

    #[test]
    fn the_longest_prefix_wins_and_the_first_listed_of_equals() {
        let equal = ["@a/*", "@a/*.js", "@b/*"];
        assert_eq!(
            first_longest(equal.iter(), |pattern| prefix_len(pattern)),
            Some(&"@a/*")
        );
        let exact = ["@a/*", "@a"];
        assert_eq!(
            first_longest(exact.iter(), |pattern| prefix_len(pattern)),
            Some(&"@a")
        );
    }

    #[test]
    fn an_extensionless_path_is_never_a_module_file() {
        assert_eq!(
            tried("a/x", false),
            [
                "a/x",
                "a/x.ts",
                "a/x.tsx",
                "a/x.d.ts",
                "a/x.js",
                "a/x.jsx",
                "a/x/index.ts",
                "a/x/index.tsx",
                "a/x/index.d.ts",
                "a/x/index.js",
                "a/x/index.jsx"
            ]
        );
    }

    #[test]
    fn a_built_extension_tries_the_sources_it_may_come_from() {
        assert_eq!(tried("x.mjs", false), ["x.mts", "x.d.mts", "x.mjs"]);
        assert_eq!(tried("x.cjs", false), ["x.cts", "x.d.cts", "x.cjs"]);
        assert_eq!(
            tried("x.js", false),
            ["x.ts", "x.tsx", "x.d.ts", "x.js", "x.jsx"]
        );
        assert_eq!(tried("x.jsx", false), ["x.tsx", "x.jsx"]);
    }

    #[test]
    fn a_source_and_a_directory_try_one_kind_of_thing() {
        assert_eq!(tried("a/x.d.ts", false), ["a/x.d.ts"]);
        assert_eq!(
            tried("a", true).first().map(String::as_str),
            Some("a/index.ts")
        );
        assert_eq!(
            tried("", true).first().map(String::as_str),
            Some("index.ts")
        );
    }

    #[test]
    fn the_first_active_condition_decides_even_when_it_is_null() {
        let map = crate::json::parse(r#"{"import": null, "default": "./x.js"}"#).unwrap();
        let configured = ["default".to_owned()];
        let import = Conditions {
            configured: &configured,
            form: "import",
        };
        assert_eq!(pick_target(&map, &import), Some(Picked::Blocked));
        let require = Conditions {
            configured: &configured,
            form: "require",
        };
        assert_eq!(pick_target(&map, &require), Some(Picked::Path("./x.js")));
    }

    proptest! {
        #[test]
        fn capturing_never_panics(pattern in "[a-z*/@]{0,10}", spec in "[a-z/@]{0,10}") {
            let _ = capture(&pattern, &spec);
        }

        #[test]
        fn trying_candidates_never_panics(path in "[a-z./]{0,12}", directory in any::<bool>()) {
            prop_assert!(tried(&path, directory).len() <= 11);
        }
    }
}
