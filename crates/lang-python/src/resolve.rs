//! Where an import points: files of the repository, the standard library, a declared dependency or nothing.
//!
//! Resolution never touches the disk. It asks the project's view of the repository for one candidate path at a time,
//! and looks for a module in this order, the first place that has it winning:
//!
//! 1. the project's own source roots, as a package or a module;
//! 2. the standard library;
//! 3. the source roots of the other Python projects of the repository, as a package or a module;
//! 4. the distributions the project declares;
//! 5. a namespace package, under the project's own source roots and then under the others'.
//!
//! A namespace package comes last because it is only a directory. Python takes a regular package or a module of the
//! same name from anywhere on its path before it, so a folder called `logging` beside the code never hides the
//! standard library.

use gsttt_core::fact::{Import, Resolution, UnresolvedReason};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use ruff_python_stdlib::sys::{is_builtin_module, is_known_standard_library};

use crate::files::is_python_source;
use crate::names::distributions_for_import;
use crate::project::PythonProject;

/// The source and the stub of one module or package, whichever of the two exist. Both are reached: the source is
/// what runs, and a type checker reads the stub in its place.
#[derive(Clone, Copy, Debug)]
struct Files {
    source: Option<FileId>,
    stub: Option<FileId>,
}

impl Files {
    /// `None` when neither file exists.
    fn of(source: Option<FileId>, stub: Option<FileId>) -> Option<Self> {
        (source.is_some() || stub.is_some()).then_some(Self { source, stub })
    }

    /// The file that holds the code, when there is one.
    fn primary(self) -> Option<FileId> {
        self.source.or(self.stub)
    }

    fn iter(self) -> impl Iterator<Item = FileId> {
        self.source.into_iter().chain(self.stub)
    }
}

/// What a name is inside a directory.
#[derive(Clone, Copy, Debug)]
enum Spot {
    /// A directory with an `__init__` file: a regular package.
    Package(Files),
    /// A `.py` or `.pyi` file.
    Module(Files),
    /// A directory with Python files below it and no `__init__` file: a namespace package.
    Namespace,
    Missing,
}

/// One import to find: the file that writes it, the dotted path of the module and the names taken from it.
#[derive(Clone, Copy, Debug)]
struct Query<'a> {
    from: &'a RepoPath,
    module: &'a str,
    names: &'a [String],
}

/// What a module path comes to under one directory.
#[derive(Debug)]
enum Reach {
    /// Every segment is there.
    Found {
        /// The files the import runs or reads: the packages it passes through, the module, and the submodules among
        /// its names. The packages the importing file sits in are not among them, for they are initialised before
        /// the file runs.
        files: Vec<FileId>,
        /// The file of the module the path names, even when `files` leaves it out.
        target: Option<FileId>,
        /// The first segment is a namespace package, so the path loses to a regular package of the same name.
        namespace: bool,
    },
    /// The first segment is there and the rest is not: a typo, or a module that was deleted.
    Partial,
    Absent,
}

/// Looks the candidate files of a module path up in the repository, building each path in one buffer.
struct Probe<'a> {
    repo: &'a RepoView,
    /// The directory or file being looked at. `reach` sets it and `enter` goes deeper; the lookups put it back as they found it.
    path: String,
}

impl<'a> Probe<'a> {
    fn new(repo: &'a RepoView) -> Self {
        Self {
            repo,
            path: String::with_capacity(128),
        }
    }

    /// Goes into `name`: `a` and `b` make `a/b`, and the root's empty path and `b` make `b`.
    fn enter(&mut self, name: &str) {
        if !self.path.is_empty() {
            self.path.push('/');
        }
        self.path.push_str(name);
    }

    /// The file `name` inside the directory the path names.
    fn child(&mut self, name: &str) -> Option<FileId> {
        let end = self.path.len();
        self.enter(name);
        let file = self.repo.lookup(&self.path);
        self.path.truncate(end);
        file
    }

    /// The file the path names with `extension` added.
    fn with_extension(&mut self, extension: &str) -> Option<FileId> {
        let end = self.path.len();
        self.path.push_str(extension);
        let file = self.repo.lookup(&self.path);
        self.path.truncate(end);
        file
    }

    fn package(&mut self) -> Option<Files> {
        Files::of(self.child("__init__.py"), self.child("__init__.pyi"))
    }

    fn module(&mut self) -> Option<Files> {
        Files::of(self.with_extension(".py"), self.with_extension(".pyi"))
    }

    /// Whether the directory the path names holds Python files at any depth.
    fn holds_python(&self) -> bool {
        self.repo.is_dir(&self.path)
            && self
                .repo
                .under(&self.path)
                .any(|(_, path)| is_python_source(path))
    }

    /// What the directory the path names is.
    fn directory(&mut self) -> Spot {
        if let Some(init) = self.package() {
            Spot::Package(init)
        } else if self.holds_python() {
            Spot::Namespace
        } else {
            Spot::Missing
        }
    }

    /// What `name` is inside the directory the path names. A package comes before a module of the same name, as
    /// Python's finder takes it, and a namespace package is only what is left when neither is there.
    fn spot(&mut self, name: &str) -> Spot {
        let outer = self.path.len();
        self.enter(name);
        let spot = if let Some(init) = self.package() {
            Spot::Package(init)
        } else if let Some(module) = self.module() {
            Spot::Module(module)
        } else if self.holds_python() {
            Spot::Namespace
        } else {
            Spot::Missing
        };
        self.path.truncate(outer);
        spot
    }

    /// What `from <module> import <names>` of the file `query.from` reaches below the directory `base`.
    ///
    /// An import runs every package on the way to its module, so their `__init__` files are reached too, except
    /// those of the packages the importing file sits in. A name that is a submodule is imported as the module file;
    /// a name that is not is an attribute of the module, so the module's own file is reached. When every name is a
    /// submodule and the importing file sits in the module's package, the package's file is not.
    fn reach(&mut self, query: Query<'_>, base: &str) -> Reach {
        self.path.clear();
        self.path.push_str(base);
        let mut files = Vec::new();
        let mut first = None;
        let mut target = Spot::Missing;
        let mut segments = query
            .module
            .split('.')
            .filter(|segment| !segment.is_empty())
            .peekable();
        // `from . import x`: the module is the package the dots name.
        if segments.peek().is_none() {
            target = self.directory();
            first = Some(target);
        }
        while let Some(segment) = segments.next() {
            let spot = self.spot(segment);
            let head = *first.get_or_insert(spot);
            self.enter(segment);
            let last = segments.peek().is_none();
            match spot {
                Spot::Missing => return miss(head),
                // A module has no submodules.
                Spot::Module(_) if !last => return Reach::Partial,
                Spot::Package(init) if !last && !query.from.is_under(&self.path) => {
                    files.extend(init.iter());
                }
                Spot::Package(_) | Spot::Module(_) | Spot::Namespace => {}
            }
            target = spot;
        }
        if matches!(target, Spot::Missing) {
            return Reach::Absent;
        }

        // `*` imports what the module's file defines, never a submodule.
        let mut named = 0_usize;
        let mut submodules = 0_usize;
        let names = query
            .names
            .iter()
            .map(String::as_str)
            .filter(|name| *name != "*");
        for name in names {
            named += 1;
            if matches!(target, Spot::Package(_) | Spot::Namespace)
                && let Spot::Package(init) | Spot::Module(init) = self.spot(name)
            {
                submodules += 1;
                files.extend(init.iter());
            }
        }
        let only_submodules = named > 0 && submodules == named;
        if let Spot::Package(init) | Spot::Module(init) = target
            && !(only_submodules && query.from.is_under(&self.path))
        {
            files.extend(init.iter());
        }
        files.sort_unstable();
        files.dedup();
        // A namespace package holds no file of its own. Importing it is fine; importing a name it does not have is
        // not.
        if files.is_empty() && named > 0 {
            return Reach::Partial;
        }
        Reach::Found {
            files,
            target: match target {
                Spot::Package(init) | Spot::Module(init) => init.primary(),
                Spot::Namespace | Spot::Missing => None,
            },
            namespace: matches!(first, Some(Spot::Namespace)),
        }
    }
}

/// What a path that stopped short comes to, given what its first segment is: a first segment that is there makes
/// the rest a typo.
fn miss(head: Spot) -> Reach {
    match head {
        Spot::Missing => Reach::Absent,
        Spot::Package(_) | Spot::Module(_) | Spot::Namespace => Reach::Partial,
    }
}

/// Where a module path points.
#[derive(Debug)]
enum Hit {
    /// Files of the repository, and the one among them that is the module the path names.
    Files {
        files: Vec<FileId>,
        target: Option<FileId>,
    },
    Builtin,
    External(String),
    Missing(UnresolvedReason),
}

/// What the search of the source roots passed on its way, for when nothing better turns up.
#[derive(Default)]
struct Seen {
    /// The first segment of the path is a package or a directory of the repository, so a path that does not resolve
    /// is a typo and no library.
    first_party: bool,
    /// The first hit under a namespace package.
    namespace: Option<Hit>,
}

impl Seen {
    /// The hit under the first of `roots` that holds the module as a regular package or module. A namespace package
    /// is kept for later.
    fn search(&mut self, probe: &mut Probe<'_>, roots: &[String], query: Query<'_>) -> Option<Hit> {
        for base in roots {
            match probe.reach(query, base) {
                Reach::Found {
                    files,
                    target,
                    namespace: false,
                } => return Some(Hit::Files { files, target }),
                Reach::Found {
                    files,
                    target,
                    namespace: true,
                } => {
                    self.first_party = true;
                    self.namespace.get_or_insert(Hit::Files { files, target });
                }
                Reach::Partial => self.first_party = true,
                Reach::Absent => {}
            }
        }
        None
    }
}

/// Where the import of `spec` with `names` in the file `from` points.
fn locate(project: &PythonProject, from: &RepoPath, spec: &str, names: &[String]) -> Hit {
    let dots = spec.bytes().take_while(|byte| *byte == b'.').count();
    let module = spec.get(dots..).unwrap_or_default();
    let query = Query {
        from,
        module,
        names,
    };
    let mut probe = Probe::new(&project.repo);

    // A relative import starts from the importing file's package whatever the source roots are.
    if dots > 0 {
        return match package_dir(from, dots).map(|base| probe.reach(query, base)) {
            Some(Reach::Found { files, target, .. }) => Hit::Files { files, target },
            _ => Hit::Missing(UnresolvedReason::NotFound),
        };
    }
    if module.is_empty() {
        return Hit::Missing(UnresolvedReason::NotFound);
    }

    let mut seen = Seen::default();
    if let Some(hit) = seen.search(&mut probe, &project.source_roots, query) {
        return hit;
    }
    if is_standard_library(module.split('.').next().unwrap_or_default()) {
        return Hit::Builtin;
    }
    if let Some(hit) = seen.search(&mut probe, &project.sibling_roots, query) {
        return hit;
    }
    if let Some(package) = distributions_for_import(module)
        .into_iter()
        .find(|name| project.dependencies.contains(name))
    {
        return Hit::External(package);
    }
    seen.namespace.unwrap_or(Hit::Missing(if seen.first_party {
        UnresolvedReason::NotFound
    } else {
        UnresolvedReason::UnknownPackage
    }))
}

/// Where `import` of the file `from` points.
pub(crate) fn resolve(project: &PythonProject, from: &RepoPath, import: &Import) -> Resolution {
    match locate(project, from, &import.spec, &import.names) {
        Hit::Files { files, .. } => found(files),
        Hit::Builtin => Resolution::Builtin,
        Hit::External(package) => Resolution::External { package },
        Hit::Missing(reason) => Resolution::Unresolved(reason),
    }
}

/// The file of the module `spec` names for the file `from`, when it is a file of the repository: the module a
/// router lives in, found the way an import of it would be.
pub(crate) fn module_file(project: &PythonProject, from: &RepoPath, spec: &str) -> Option<FileId> {
    match locate(project, from, spec, &[]) {
        Hit::Files { target, .. } => target,
        Hit::Builtin | Hit::External(_) | Hit::Missing(_) => None,
    }
}

/// The directory `level` dots start from: one dot is the package of the file, each further dot its parent.
fn package_dir(from: &RepoPath, level: usize) -> Option<&str> {
    let mut dir = from.parent();
    for _ in 1..level {
        if dir.is_empty() {
            return None;
        }
        dir = dir.rsplit_once('/').map_or("", |(parent, _)| parent);
    }
    Some(dir)
}

/// The resolution of an import that reaches `files`. An import that reaches none names a namespace package, which
/// has no file to depend on.
fn found(files: Vec<FileId>) -> Resolution {
    match files.as_slice() {
        [] => Resolution::Unresolved(UnresolvedReason::NotFound),
        [file] => Resolution::File { file: *file },
        _ => Resolution::Files { files },
    }
}

/// Whether `top` is a standard-library module of some Python from 3.7 on. The project does not say which Python
/// it runs, so a module that any of them ships counts.
fn is_standard_library(top: &str) -> bool {
    (7..=15).any(|minor| is_known_standard_library(minor, top) || is_builtin_module(minor, top))
}

#[cfg(test)]
mod tests {
    use gsttt_core::fact::ImportKind;
    use proptest::prelude::*;

    use super::*;
    use crate::PythonOptions;
    use crate::project::open_projects;

    /// The project at `root` of a repository made of `paths`, declaring `dependencies`.
    fn project(paths: &[&str], root: &str, dependencies: &[&str]) -> PythonProject {
        let files = paths
            .iter()
            .map(|path| RepoPath::normalize(path).unwrap())
            .collect();
        // Nothing is read from disk: a manifest that cannot be read is skipped, and the dependencies are set below.
        let repo = RepoView::new("/nonexistent", files);
        let mut project = open_projects(&repo, &PythonOptions::default(), &[])
            .unwrap()
            .into_iter()
            .find(|project| project.root == root)
            .unwrap();
        project.dependencies = dependencies.iter().map(|name| (*name).to_owned()).collect();
        project
    }

    fn import(spec: &str, names: &[&str]) -> Import {
        let mut import = Import::new(spec, ImportKind::Static, 1);
        import.names = names.iter().map(|name| (*name).to_owned()).collect();
        import
    }

    /// What the import resolves to, as these tests read it: the paths of its files joined by ` + `, `builtin`,
    /// `package <name>` or `unresolved <reason>`.
    fn shown(project: &PythonProject, from: &str, spec: &str, names: &[&str]) -> String {
        let from = RepoPath::normalize(from).unwrap();
        let paths = |files: &[FileId]| {
            files
                .iter()
                .map(|file| project.repo.path(*file).unwrap().as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        };
        match resolve(project, &from, &import(spec, names)) {
            Resolution::File { file } => paths(&[file]),
            Resolution::Files { files } => paths(&files),
            Resolution::External { package } => format!("package {package}"),
            Resolution::Builtin => "builtin".to_owned(),
            Resolution::Unresolved(reason) => format!("unresolved {reason:?}"),
        }
    }

    /// Every case is `(module, names, what it resolves to)`, imported by `from`.
    fn assert_resolves(project: &PythonProject, from: &str, cases: &[(&str, &[&str], &str)]) {
        for (spec, names, want) in cases {
            assert_eq!(
                shown(project, from, spec, names),
                *want,
                "{from} imports {spec} {names:?}"
            );
        }
    }

    #[test]
    fn a_package_comes_before_a_module_of_the_same_name() {
        let project = project(
            &[
                "pyproject.toml",
                "both.py",
                "both/__init__.py",
                "a/__init__.py",
                "a/b.py",
                "a/b/__init__.py",
                "main.py",
            ],
            "",
            &[],
        );
        assert_resolves(
            &project,
            "main.py",
            &[
                ("both", &[], "both/__init__.py"),
                ("a.b", &[], "a/__init__.py + a/b/__init__.py"),
                ("a", &["b"], "a/__init__.py + a/b/__init__.py"),
            ],
        );
    }

    #[test]
    fn a_stub_is_reached_with_its_source_and_alone() {
        let project = project(
            &[
                "pyproject.toml",
                "m.py",
                "m.pyi",
                "only.pyi",
                "p/__init__.py",
                "p/__init__.pyi",
                "stubs/__init__.pyi",
                "main.py",
            ],
            "",
            &[],
        );
        assert_resolves(
            &project,
            "main.py",
            &[
                ("m", &[], "m.py + m.pyi"),
                ("only", &[], "only.pyi"),
                ("p", &[], "p/__init__.py + p/__init__.pyi"),
                ("stubs", &[], "stubs/__init__.pyi"),
            ],
        );
    }

    #[test]
    fn the_file_of_a_module_is_its_source_before_its_stub() {
        let project = project(
            &[
                "pyproject.toml",
                "m.py",
                "m.pyi",
                "only.pyi",
                "p/__init__.py",
                "main.py",
            ],
            "",
            &["requests"],
        );
        let from = RepoPath::normalize("main.py").unwrap();
        let file = |path: &str| project.repo.lookup(path);
        assert_eq!(module_file(&project, &from, "m"), file("m.py"));
        assert_eq!(module_file(&project, &from, "only"), file("only.pyi"));
        assert_eq!(module_file(&project, &from, "p"), file("p/__init__.py"));
        assert_eq!(module_file(&project, &from, "requests"), None);
        assert_eq!(module_file(&project, &from, "os"), None);
        assert_eq!(module_file(&project, &from, "missing"), None);
    }

    #[test]
    fn an_import_reaches_the_packages_it_passes_through() {
        let project = project(
            &[
                "pyproject.toml",
                "pkg/__init__.py",
                "pkg/other.py",
                "pkg/sub/__init__.py",
                "pkg/sub/leaf.py",
                "outside.py",
            ],
            "",
            &[],
        );
        let chain = "pkg/__init__.py + pkg/sub/__init__.py + pkg/sub/leaf.py";
        assert_resolves(
            &project,
            "outside.py",
            &[
                ("pkg.sub.leaf", &[], chain),
                ("pkg.sub", &["leaf"], chain),
                // An attribute lives in the package's own file.
                (
                    "pkg.sub",
                    &["value"],
                    "pkg/__init__.py + pkg/sub/__init__.py",
                ),
                ("pkg", &["sub"], "pkg/__init__.py + pkg/sub/__init__.py"),
                ("pkg", &["*"], "pkg/__init__.py"),
            ],
        );
        // A file inside `pkg` runs after `pkg/__init__.py` has run.
        assert_resolves(
            &project,
            "pkg/other.py",
            &[
                ("pkg.sub.leaf", &[], "pkg/sub/__init__.py + pkg/sub/leaf.py"),
                ("pkg", &["sub"], "pkg/sub/__init__.py"),
                ("pkg", &["other"], "pkg/other.py"),
                // The attribute is still read from the package's file.
                ("pkg", &["value"], "pkg/__init__.py"),
            ],
        );
        assert_resolves(
            &project,
            "pkg/sub/leaf.py",
            &[
                ("pkg.sub.leaf", &[], "pkg/sub/leaf.py"),
                ("pkg.sub", &["leaf"], "pkg/sub/leaf.py"),
                ("pkg.sub", &[], "pkg/sub/__init__.py"),
            ],
        );
    }

    #[test]
    fn relative_imports_start_at_the_package_of_the_file() {
        let project = project(
            &[
                "pyproject.toml",
                "pkg/__init__.py",
                "pkg/a.py",
                "pkg/sub/__init__.py",
                "pkg/sub/b.py",
                "pkg/sub/c.py",
                "top.py",
            ],
            "",
            &[],
        );
        assert_resolves(
            &project,
            "pkg/sub/b.py",
            &[
                (".c", &[], "pkg/sub/c.py"),
                (".", &["c"], "pkg/sub/c.py"),
                ("..a", &[], "pkg/a.py"),
                ("..", &["a"], "pkg/a.py"),
                ("..", &["value"], "pkg/__init__.py"),
                // The repository root holds Python files and no `__init__.py`: a namespace.
                ("...", &["top"], "top.py"),
                (".missing", &[], "unresolved NotFound"),
                // One dot too many leaves the repository.
                ("....", &["top"], "unresolved NotFound"),
            ],
        );
    }

    #[test]
    fn a_module_has_no_submodules() {
        let project = project(&["pyproject.toml", "m.py", "main.py"], "", &[]);
        assert_resolves(
            &project,
            "main.py",
            &[
                ("m", &[], "m.py"),
                ("m.sub", &[], "unresolved NotFound"),
                ("missing", &[], "unresolved UnknownPackage"),
            ],
        );
    }

    #[test]
    fn a_namespace_directory_comes_after_the_standard_library_and_the_declared_distributions() {
        let project = project(
            &[
                "pyproject.toml",
                "logging/formatters.py",
                "types/aliases.py",
                "yaml/loaders.py",
                "ns/part.py",
                "main.py",
            ],
            "",
            &["pyyaml"],
        );
        assert_resolves(
            &project,
            "main.py",
            &[
                ("logging", &[], "builtin"),
                ("logging.handlers", &[], "builtin"),
                ("types", &[], "builtin"),
                ("yaml", &[], "package pyyaml"),
                ("yaml.nodes", &[], "package pyyaml"),
                ("ns.part", &[], "ns/part.py"),
                ("ns", &["part"], "ns/part.py"),
                // A directory is no file: importing it reaches nothing, and neither does a name it lacks.
                ("ns", &[], "unresolved NotFound"),
                ("ns", &["*"], "unresolved NotFound"),
                ("ns", &["lacking"], "unresolved NotFound"),
                ("ns.missing", &[], "unresolved NotFound"),
            ],
        );
    }

    #[test]
    fn a_regular_package_of_the_project_beats_the_standard_library() {
        let project = project(
            &["pyproject.toml", "logging/__init__.py", "main.py"],
            "",
            &[],
        );
        assert_resolves(
            &project,
            "main.py",
            &[("logging", &[], "logging/__init__.py")],
        );
    }

    #[test]
    fn the_libraries_of_other_projects_come_before_the_declared_distributions() {
        let paths = [
            "libs/common/pyproject.toml",
            "libs/common/common/__init__.py",
            "libs/common/common/util.py",
            "services/api/pyproject.toml",
            "services/api/app/__init__.py",
            "services/api/app/main.py",
        ];
        let api = project(&paths, "services/api", &["common", "fastapi"]);
        assert_resolves(
            &api,
            "services/api/app/main.py",
            &[
                (
                    "common.util",
                    &[],
                    "libs/common/common/__init__.py + libs/common/common/util.py",
                ),
                ("common", &[], "libs/common/common/__init__.py"),
                ("app", &[], "services/api/app/__init__.py"),
                ("fastapi", &[], "package fastapi"),
                ("requests", &[], "unresolved UnknownPackage"),
            ],
        );
    }

    #[test]
    fn a_project_finds_its_own_modules_before_those_of_a_neighbour() {
        let paths = [
            "a/pyproject.toml",
            "a/main.py",
            "a/shared.py",
            "b/pyproject.toml",
            "b/shared.py",
            "b/only_b.py",
        ];
        let a = project(&paths, "a", &[]);
        assert_resolves(
            &a,
            "a/main.py",
            &[
                ("shared", &[], "a/shared.py"),
                ("only_b", &[], "b/only_b.py"),
            ],
        );
        let b = project(&paths, "b", &[]);
        assert_resolves(&b, "b/shared.py", &[("shared", &[], "b/shared.py")]);
    }

    #[test]
    fn a_regular_package_of_a_neighbour_beats_a_namespace_directory_of_the_project() {
        let paths = [
            "a/pyproject.toml",
            "a/main.py",
            "a/util/one.py",
            "b/pyproject.toml",
            "b/util/__init__.py",
            "b/util/two.py",
        ];
        let a = project(&paths, "a", &[]);
        assert_resolves(
            &a,
            "a/main.py",
            &[
                ("util", &[], "b/util/__init__.py"),
                ("util.two", &[], "b/util/__init__.py + b/util/two.py"),
                // The directory is all that has `one`, so it is used after all.
                ("util.one", &[], "a/util/one.py"),
            ],
        );
    }

    #[test]
    fn namespace_portions_are_joined_across_projects_the_project_first() {
        let paths = [
            "a/pyproject.toml",
            "a/main.py",
            "a/ns/one.py",
            "a/ns/same.py",
            "b/pyproject.toml",
            "b/ns/two.py",
            "b/ns/same.py",
        ];
        let a = project(&paths, "a", &[]);
        assert_resolves(
            &a,
            "a/main.py",
            &[
                ("ns.one", &[], "a/ns/one.py"),
                ("ns.two", &[], "b/ns/two.py"),
                ("ns.same", &[], "a/ns/same.py"),
                ("ns.three", &[], "unresolved NotFound"),
            ],
        );
    }

    #[test]
    fn a_namespace_above_a_package_adds_nothing_of_its_own() {
        let project = project(
            &[
                "pyproject.toml",
                "ns/pkg/__init__.py",
                "ns/pkg/mod.py",
                "main.py",
            ],
            "",
            &[],
        );
        assert_resolves(
            &project,
            "main.py",
            &[
                ("ns.pkg", &[], "ns/pkg/__init__.py"),
                ("ns.pkg.mod", &[], "ns/pkg/__init__.py + ns/pkg/mod.py"),
            ],
        );
    }

    #[test]
    fn src_is_searched_before_the_project_directory() {
        let project = project(
            &[
                "pyproject.toml",
                "src/booking/__init__.py",
                "booking/__init__.py",
                "tests/test_api.py",
            ],
            "",
            &[],
        );
        assert_eq!(project.source_roots, ["src", ""]);
        assert_resolves(
            &project,
            "tests/test_api.py",
            &[("booking", &[], "src/booking/__init__.py")],
        );
    }

    proptest! {
        #[test]
        fn resolution_lists_each_file_of_the_repository_once_in_order(
            dots in 0_usize..5,
            segments in proptest::collection::vec(
                prop::sample::select(vec![
                    "pkg", "sub", "leaf", "both", "logging", "yaml", "ns", "part", "m", "missing", "",
                ]),
                0..5,
            ),
            names in proptest::collection::vec(
                prop::sample::select(vec!["sub", "leaf", "part", "value", "*", ""]),
                0..4,
            ),
            from in prop::sample::select(vec![
                "main.py",
                "pkg/other.py",
                "pkg/sub/leaf.py",
                "ns/part.py",
            ]),
        ) {
            let project = project(
                &[
                    "pyproject.toml",
                    "main.py",
                    "both.py",
                    "both/__init__.py",
                    "m.py",
                    "m.pyi",
                    "pkg/__init__.py",
                    "pkg/other.py",
                    "pkg/sub/__init__.py",
                    "pkg/sub/leaf.py",
                    "logging/formatters.py",
                    "yaml/loaders.py",
                    "ns/part.py",
                ],
                "",
                &["pyyaml"],
            );
            let spec = format!("{}{}", ".".repeat(dots), segments.join("."));
            let from = RepoPath::normalize(from).unwrap();
            let names: Vec<&str> = names;
            match resolve(&project, &from, &import(&spec, &names)) {
                Resolution::File { file } => prop_assert!(project.repo.path(file).is_some()),
                Resolution::Files { files } => {
                    prop_assert!(files.len() > 1);
                    prop_assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
                    prop_assert!(files.iter().all(|file| project.repo.path(*file).is_some()));
                }
                Resolution::External { .. } | Resolution::Builtin | Resolution::Unresolved(_) => {}
            }
        }
    }
}
