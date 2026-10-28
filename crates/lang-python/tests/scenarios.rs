//! The scenarios of the `python` capability. Each test indexes one fixture tree from `tests/fixtures/` through the
//! whole pipeline, checks what its scenario claims and snapshots the facts it is about, for review.
//!
//! `cfg(test)` on the whole file tells clippy that the helpers below are test code, where `clippy.toml` lets them panic.

#![cfg(test)]

use std::collections::HashMap;
use std::fmt::Write;

use gsttt_config::pack::builtin_packs;
use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::engine::pipeline::{self, FileRun, Module, Output};
use gsttt_core::fact::{
    ContractFact, Embedded, FileTags, ImportFlags, ImportKind, Line, ParseStatus, Resolution,
};
use gsttt_core::ids::FileId;
use gsttt_core::repo::RepoView;
use gsttt_core::rules::Pack;
use gsttt_lang_python::{PACKS, PYTHON};
use serde_json::{Value, json};

/// Larger than any fixture file.
const MAX_FILE_BYTES: u64 = 1 << 20;

/// A fixture tree after the pipeline ran on it, with the output as JSON for the snapshots.
struct Tree {
    repo: RepoView,
    output: Output,
    json: Value,
    /// The directory an inline tree was written to, kept until the test ends.
    _dir: Option<tempfile::TempDir>,
}

fn index(fixture: &str) -> Tree {
    index_with(fixture, builtin_packs(PACKS).unwrap())
}

/// Like [`index`], with `packs` in place of the built-in ones.
fn index_with(fixture: &str, packs: Vec<Pack>) -> Tree {
    let root = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    run(root, packs, None)
}

/// A tree written to a temporary directory from `(path, text)` pairs and indexed like a fixture, for the inputs that
/// fit in a few lines.
fn index_files(files: &[(&str, &str)]) -> Tree {
    index_files_with(files, builtin_packs(PACKS).unwrap())
}

/// Like [`index_files`], with `packs` in place of the built-in ones.
fn index_files_with(files: &[(&str, &str)], packs: Vec<Pack>) -> Tree {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let root = dir.path().to_str().unwrap().to_owned();
    run(root, packs, Some(dir))
}

fn run(root: String, packs: Vec<Pack>, dir: Option<tempfile::TempDir>) -> Tree {
    let repo = RepoView::scan(root).unwrap();
    let modules = [Module {
        connector: &PYTHON,
        options: Value::Null,
        packs,
    }];
    let output = pipeline::run(&repo, &modules, MAX_FILE_BYTES).unwrap();
    let json = output.to_json(&repo);
    Tree {
        repo,
        output,
        json,
        _dir: dir,
    }
}

/// A contract fact as `<role> <key>@<line>`, the key carrying its kind.
fn show(fact: &ContractFact) -> String {
    format!("{} {}@{}", fact.role, fact.key, fact.line)
}

impl Tree {
    fn path_of(&self, id: FileId) -> &str {
        self.repo
            .path(id)
            .expect("the id names a file of the tree")
            .as_str()
    }

    fn file(&self, path: &str) -> &FileRun {
        self.output
            .files
            .iter()
            .find(|file| self.path_of(file.id) == path)
            .expect("the fixture holds the file")
    }

    /// Whether a project claimed the file at `path`.
    fn claims(&self, path: &str) -> bool {
        self.output
            .files
            .iter()
            .any(|file| self.path_of(file.id) == path)
    }

    /// The JSON of one file, the form its snapshot takes.
    fn file_json(&self, path: &str) -> &Value {
        self.json["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == path)
            .expect("the output holds the file")
    }

    /// One field of every file, by path.
    fn column(&self, field: &str) -> Value {
        Value::Object(
            self.json["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|file| {
                    (
                        file["path"].as_str().unwrap().to_owned(),
                        file[field].clone(),
                    )
                })
                .collect(),
        )
    }

    fn roots(&self) -> Vec<&str> {
        self.output
            .projects
            .iter()
            .map(|project| project.root.as_str())
            .collect()
    }

    /// Where the import of `spec` with `names` in the file at `path` points, in words.
    fn target(&self, path: &str, spec: &str, names: &[&str]) -> String {
        let resolved = self
            .file(path)
            .imports
            .iter()
            .find(|resolved| resolved.import.spec == spec && resolved.import.names == names)
            .expect("the file has the import");
        match &resolved.to {
            Resolution::File { file } => self.path_of(*file).to_owned(),
            Resolution::Files { files } => files
                .iter()
                .map(|file| self.path_of(*file))
                .collect::<Vec<_>>()
                .join(" + "),
            Resolution::External { package } => format!("package {package}"),
            Resolution::Builtin => "builtin".to_owned(),
            Resolution::Unresolved(reason) => format!("unresolved {reason:?}"),
        }
    }

    /// The contract facts of a file, in its order.
    fn contracts(&self, path: &str) -> Vec<String> {
        self.file(path).facts.contracts.iter().map(show).collect()
    }

    /// The lines of the sites of a file that were found and could not be read.
    fn unresolved_lines(&self, path: &str) -> Vec<Line> {
        let unresolved = &self.file(path).facts.unresolved;
        unresolved.iter().map(|site| site.line).collect()
    }

    /// The facts `link` added to a file.
    fn linked(&self, path: &str) -> Vec<String> {
        self.output
            .linked
            .iter()
            .filter(|linked| self.path_of(linked.file) == path)
            .map(|linked| show(&linked.fact))
            .collect()
    }

    /// The unread sites of a file, as `L<line> <what>`.
    fn unread(&self, path: &str) -> Vec<String> {
        self.file(path)
            .facts
            .unresolved
            .iter()
            .map(|site| format!("L{} {}", site.line, site.detail))
            .collect()
    }

    /// Everything the first project says about what it cannot see.
    fn blind_spots(&self) -> Vec<&str> {
        self.output
            .projects
            .first()
            .expect("the tree opened a project")
            .provides
            .iter()
            .flat_map(|provide| provide.blind_spots.iter().map(String::as_str))
            .collect()
    }
}

#[gsttt_scenario::scenario("python", "Each pyproject is a project")]
#[test]
fn each_pyproject_is_a_project() {
    let tree = index("workspace");
    let roots = tree.roots();
    assert!(roots.contains(&"services/api"), "{roots:?}");
    assert!(roots.contains(&"services/worker"), "{roots:?}");
    assert_eq!(
        tree.file_json("services/api/app/main.py")["project"],
        "python services/api"
    );
    assert_eq!(
        tree.file_json("services/worker/worker/main.py")["project"],
        "python services/worker"
    );
    insta::assert_json_snapshot!(
        "each_pyproject_is_a_project",
        json!({ "roots": roots, "project_of": tree.column("project") })
    );
}

#[gsttt_scenario::scenario(
    "python",
    "The root project opens only for Python files outside every project"
)]
#[test]
fn the_root_project_opens_only_for_python_files_outside_every_project() {
    // Every Python file is under a manifest: an empty root project would only be noise in the coverage.
    assert_eq!(
        index("workspace").roots(),
        ["services/api", "services/worker"]
    );
    // `tools/release.py` is under none of them, so the root project reads it.
    let stray = index("root_script");
    assert_eq!(stray.roots(), ["", "services/api"]);
    assert_eq!(stray.file_json("tools/release.py")["project"], "python .");
    assert_eq!(
        stray.file_json("services/api/app.py")["project"],
        "python services/api"
    );
}

#[gsttt_scenario::scenario("python", "Requirements files include each other")]
#[test]
fn requirements_files_include_each_other() {
    let tree = index("requirement_includes");
    // `fastapi` is declared two includes away from `requirements.txt` and `sqlalchemy` three, each include relative to
    // the file that names it. `common/shared.txt` includes `requirements/base.txt` back, and `requirements/dev.txt`
    // starts with two includes that name nothing in the repository: the walk reads each file once and goes on.
    assert_eq!(tree.contracts("app.py"), ["serves http GET /ping@6"]);
    assert_eq!(tree.contracts("models.py"), ["declares table bookings@5"]);
    // Nothing in the name of `common/shared.txt` says it is a manifest, so it is read through the include and no more.
    assert!(!tree.claims("common/shared.txt"));
}

#[gsttt_scenario::scenario("python", "A requirements directory holds manifests")]
#[test]
fn a_requirements_directory_holds_manifests() {
    let tree = index("requirement_includes");
    for path in [
        "requirements.txt",
        "requirements/base.txt",
        "requirements/dev.txt",
    ] {
        assert!(
            tree.file(path).facts.tags.contains(FileTags::MANIFEST),
            "{path}"
        );
        assert_eq!(tree.file_json(path)["project"], "python .", "{path}");
    }
}

#[gsttt_scenario::scenario("python", "A malformed pyproject.toml is a failed file")]
#[test]
fn a_malformed_pyproject_is_a_failed_file() {
    let tree = index("bad_manifest");
    let manifest = &tree.file_json("pyproject.toml")["status"];
    assert_eq!(manifest["status"], "failed", "{manifest}");
    assert_eq!(manifest["failure"], "syntax", "{manifest}");
    assert_eq!(manifest["line"], 2, "{manifest}");
    assert!(
        manifest["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty()),
        "{manifest}"
    );
    // An array left open is an error at the end of the text, which is on the third and last line of the file.
    let at_the_end = &tree.file_json("lib/pyproject.toml")["status"];
    assert_eq!(at_the_end["failure"], "syntax", "{at_the_end}");
    assert_eq!(at_the_end["line"], 3, "{at_the_end}");
    // The project still opens, and the Python beside the manifest is read as it would be without the pack.
    assert_eq!(tree.file_json("app.py")["status"]["status"], "ok");
    assert_eq!(tree.file_json("app.py")["project"], "python .");
    assert!(
        tree.file("pyproject.toml")
            .facts
            .tags
            .contains(FileTags::MANIFEST)
    );
}

#[gsttt_scenario::scenario("python", "A src layout is a source root")]
#[test]
fn a_src_layout_is_a_source_root() {
    let tree = index("src_layout");
    let reached = "src/booking/__init__.py + src/booking/api.py";
    assert_eq!(
        tree.target("tests/test_api.py", "booking.api", &[]),
        reached
    );
    assert_eq!(
        tree.target("tests/test_api.py", "booking.api", &["handler"]),
        reached
    );
    insta::assert_json_snapshot!(
        "a_src_layout_is_a_source_root",
        tree.file_json("tests/test_api.py")
    );
}

#[gsttt_scenario::scenario("python", "Every import form is read")]
#[test]
fn every_import_form_is_read() {
    let tree = index("imports");
    let forms = tree.file("pkg/sub/forms.py");
    let imports: Vec<String> = forms
        .facts
        .imports
        .iter()
        .map(|import| {
            format!(
                "{} {:?} L{} {:?}",
                import.spec, import.kind, import.line, import.names
            )
        })
        .collect();
    assert_eq!(
        imports,
        [
            "importlib Static L1 []",
            "pkg.parent Static L2 []",
            "pkg.parent Static L3 []",
            "pkg.parent Static L4 [\"VALUE\"]",
            "pkg.parent Static L5 [\"VALUE\"]",
            ". Static L6 [\"sibling\"]",
            ".. Static L7 [\"parent\"]",
            "...far Static L8 [\"thing\"]",
            "pkg ReExportAll L9 [\"*\"]",
            "plugins.csv Dynamic L11 []",
            "plugins.json Dynamic L12 []",
        ]
    );
    let unresolved: Vec<u32> = forms
        .facts
        .unresolved
        .iter()
        .map(|site| site.line)
        .collect();
    assert_eq!(unresolved, [13]);
    insta::assert_json_snapshot!(
        "every_import_form_is_read",
        tree.file_json("pkg/sub/forms.py")
    );
}

/// The dynamic imports of a file as `<specifier> L<line>`, in its order.
fn dynamic_imports(tree: &Tree, path: &str) -> Vec<String> {
    let imports = &tree.file(path).facts.imports;
    imports
        .iter()
        .filter(|import| import.kind == ImportKind::Dynamic)
        .map(|import| format!("{} L{}", import.spec, import.line))
        .collect()
}

#[gsttt_scenario::scenario("python", "A relative dynamic import is taken from its package")]
#[test]
fn a_relative_dynamic_import_is_taken_from_its_package() {
    let tree = index("dynamic_imports");
    // Whichever way the arguments are written, the name is taken from the package the call names.
    assert_eq!(
        dynamic_imports(&tree, "app/named.py"),
        ["p.m L4", "p.m L5", "p.q.n L6", "p.m L7"]
    );
    insta::assert_json_snapshot!(
        "a_relative_dynamic_import_is_taken_from_its_package",
        tree.file_json("app/named.py")
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A relative dynamic import with no package is taken from the file's"
)]
#[test]
fn a_relative_dynamic_import_with_no_package_is_taken_from_the_files() {
    let tree = index("dynamic_imports");
    // A name with no leading dot has no use for the package beside it.
    assert_eq!(
        dynamic_imports(&tree, "app/own.py"),
        [".local L3", ".local L4", "p.m L5"]
    );
    assert_eq!(tree.target("app/own.py", ".local", &[]), "app/local.py");
    assert_eq!(
        tree.target("app/own.py", "p.m", &[]),
        "p/__init__.py + p/m.py"
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A dynamic import with a package that cannot be read is an unread site"
)]
#[test]
fn a_dynamic_import_with_a_package_that_cannot_be_read_is_an_unread_site() {
    let tree = index("dynamic_imports");
    // Line 3 climbs out of `p.q`, which `importlib` rejects, and line 6 has no package at all; lines 4 and 5 name
    // theirs only at run time.
    assert_eq!(tree.unresolved_lines("app/unread.py"), [3, 4, 5, 6]);
    assert_eq!(
        dynamic_imports(&tree, "app/unread.py"),
        Vec::<String>::new()
    );
}

#[gsttt_scenario::scenario("python", "Guarded imports are flagged")]
#[test]
fn guarded_imports_are_flagged() {
    let tree = index("imports");
    let guarded = tree.file("guarded.py");
    let flags = |spec: &str| {
        guarded
            .facts
            .imports
            .iter()
            .find(|import| import.spec == spec)
            .expect("the file imports it")
            .flags
    };
    assert!(flags("a").contains(ImportFlags::TYPE_CHECKING));
    assert!(!flags("a").contains(ImportFlags::OPTIONAL));
    assert!(flags("b").contains(ImportFlags::OPTIONAL));
    assert!(!flags("b").contains(ImportFlags::TYPE_CHECKING));
    assert!(flags("typing").is_empty());
    insta::assert_json_snapshot!("guarded_imports_are_flagged", tree.file_json("guarded.py"));
}

#[gsttt_scenario::scenario("python", "A recoverable error keeps the imports")]
#[test]
fn a_recoverable_error_keeps_the_imports() {
    let tree = index("imports");
    let broken = tree.file("broken.py");
    // The parser puts the error of the bracket left open at the end of the text, a line the file does not have: the
    // file has four lines, and the error is on the last of them.
    assert!(
        matches!(
            broken.facts.status,
            ParseStatus::Recovered { errors, first_error_line: 4 } if errors > 0
        ),
        "{:?}",
        broken.facts.status
    );
    let specs: Vec<&str> = broken
        .facts
        .imports
        .iter()
        .map(|import| import.spec.as_str())
        .collect();
    assert_eq!(specs, ["os", "app.db"]);
    insta::assert_json_snapshot!(
        "a_recoverable_error_keeps_the_imports",
        tree.file_json("broken.py")
    );
}

#[gsttt_scenario::scenario("python", "Test files are tagged")]
#[test]
fn test_files_are_tagged() {
    let tree = index("test_tags");
    for path in [
        "tests/helpers.py",
        "app/test_api.py",
        "app/conftest.py",
        "app/api_test.py",
        "test/util.py",
    ] {
        assert!(
            tree.file(path).facts.tags.contains(FileTags::TEST),
            "{path} is a test"
        );
    }
    assert!(!tree.file("app/api.py").facts.tags.contains(FileTags::TEST));
    insta::assert_json_snapshot!("test_files_are_tagged", tree.column("tags"));
}

#[gsttt_scenario::scenario("python", "Submodules and names are told apart")]
#[test]
fn submodules_and_names_are_told_apart() {
    let tree = index("resolution");
    assert_eq!(tree.target("app/x.py", "app", &["models"]), "app/models.py");
    assert_eq!(
        tree.target("app/x.py", "app.models", &["Booking"]),
        "app/models.py"
    );
    assert_eq!(
        tree.target("app/x.py", "app", &["settings"]),
        "app/__init__.py"
    );
    insta::assert_json_snapshot!(
        "submodules_and_names_are_told_apart",
        tree.file_json("app/x.py")
    );
}

#[gsttt_scenario::scenario("python", "Relative imports climb packages")]
#[test]
fn relative_imports_climb_packages() {
    let tree = index("resolution");
    assert_eq!(
        tree.target("app/api/routes.py", "..db", &["session"]),
        "app/db/session.py"
    );
    assert_eq!(
        tree.target("app/api/routes.py", ".", &["deps"]),
        "app/api/deps.py"
    );
    // The same imports where `db` is a module and not a package.
    let flat = index("relative_flat");
    assert_eq!(
        flat.target("app/api/routes.py", "..db", &["session"]),
        "app/db.py"
    );
    assert_eq!(
        flat.target("app/api/routes.py", ".", &["deps"]),
        "app/api/deps.py"
    );
    insta::assert_json_snapshot!(
        "relative_imports_climb_packages",
        tree.file_json("app/api/routes.py")
    );
}

#[gsttt_scenario::scenario("python", "Relative imports may climb above the project")]
#[test]
fn relative_imports_may_climb_above_the_project() {
    let tree = index("relative_above_root");
    let from = "services/api/app/x.py";
    assert_eq!(tree.target(from, "..", &["util"]), "services/api/util.py");
    assert_eq!(tree.target(from, "...", &["common"]), "services/common.py");
    assert_eq!(
        tree.target(from, "...shared", &["tool"]),
        "services/shared/tool.py"
    );
    assert_eq!(tree.target(from, "....", &["top"]), "top.py");
    assert_eq!(
        tree.target(from, ".....", &["beyond"]),
        "unresolved NotFound"
    );
}

#[gsttt_scenario::scenario("python", "Standard library, dependencies and unknown modules")]
#[test]
fn standard_library_dependencies_and_unknown_modules() {
    let tree = index("resolution");
    assert_eq!(tree.target("app/main.py", "json", &[]), "builtin");
    assert_eq!(tree.target("app/main.py", "yaml", &[]), "package pyyaml");
    assert_eq!(
        tree.target("app/main.py", "fastapi.routing", &[]),
        "package fastapi"
    );
    assert_eq!(
        tree.target("app/main.py", "requests", &[]),
        "unresolved UnknownPackage"
    );
    assert_eq!(
        tree.target("app/main.py", "app.missing", &[]),
        "unresolved NotFound"
    );
    insta::assert_json_snapshot!(
        "standard_library_dependencies_and_unknown_modules",
        tree.file_json("app/main.py")
    );
}

#[gsttt_scenario::scenario("python", "A package beats a module of the same name")]
#[test]
fn a_package_beats_a_module_of_the_same_name() {
    let tree = index("precedence");
    assert_eq!(tree.target("main.py", "both", &[]), "both/__init__.py");
    insta::assert_json_snapshot!(
        "a_package_beats_a_module_of_the_same_name",
        tree.file_json("main.py")
    );
}

#[gsttt_scenario::scenario("python", "A stub is reached with its source")]
#[test]
fn a_stub_is_reached_with_its_source() {
    let tree = index("precedence");
    assert_eq!(
        tree.target("main.py", "stubbed", &[]),
        "stubbed.py + stubbed.pyi"
    );
}

#[gsttt_scenario::scenario("python", "A library of another project resolves to its files")]
#[test]
fn a_library_of_another_project_resolves_to_its_files() {
    let tree = index("monorepo");
    let from = "services/api/app/main.py";
    let library = "libs/common/common/__init__.py + libs/common/common/util.py";
    assert_eq!(tree.target(from, "common.util", &[]), library);
    assert_eq!(tree.target(from, "common", &["util"]), library);
    // The project declares `common` too, and the files of the repository still come before the package.
    assert_eq!(tree.target(from, "fastapi", &[]), "package fastapi");
    insta::assert_json_snapshot!(
        "a_library_of_another_project_resolves_to_its_files",
        tree.file_json(from)
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A namespace directory does not shadow the standard library or a dependency"
)]
#[test]
fn a_namespace_directory_does_not_shadow_the_standard_library_or_a_dependency() {
    let tree = index("shadowing");
    assert_eq!(tree.target("main.py", "logging", &[]), "builtin");
    assert_eq!(tree.target("main.py", "logging.handlers", &[]), "builtin");
    assert_eq!(tree.target("main.py", "yaml", &[]), "package pyyaml");
    assert_eq!(tree.target("main.py", "ns.part", &[]), "ns/part.py");
    assert_eq!(
        tree.target("main.py", "ns.missing", &[]),
        "unresolved NotFound"
    );
    insta::assert_json_snapshot!(
        "a_namespace_directory_does_not_shadow_the_standard_library_or_a_dependency",
        tree.file_json("main.py")
    );
}

#[gsttt_scenario::scenario("python", "A bare namespace import names no file")]
#[test]
fn a_bare_namespace_import_names_no_file() {
    let tree = index("shadowing");
    assert_eq!(tree.target("main.py", "ns", &[]), "unresolved NotFound");
}

#[gsttt_scenario::scenario("python", "An import reaches the packages it passes through")]
#[test]
fn an_import_reaches_the_packages_it_passes_through() {
    let tree = index("passing");
    let chain = "pkg/__init__.py + pkg/sub/__init__.py + pkg/sub/leaf.py";
    assert_eq!(tree.target("outside.py", "pkg.sub.leaf", &[]), chain);
    assert_eq!(tree.target("outside.py", "pkg.sub", &["leaf"]), chain);
    assert_eq!(
        tree.target("outside.py", "pkg", &["sub"]),
        "pkg/__init__.py + pkg/sub/__init__.py"
    );
    insta::assert_json_snapshot!(
        "an_import_reaches_the_packages_it_passes_through",
        tree.file_json("outside.py")
    );
}

#[gsttt_scenario::scenario(
    "python",
    "An import inside a package adds nothing for its own package"
)]
#[test]
fn an_import_inside_a_package_adds_nothing_for_its_own_package() {
    let tree = index("passing");
    let file = "pkg/sub/sibling.py";
    assert_eq!(tree.target(file, "pkg.sub.leaf", &[]), "pkg/sub/leaf.py");
    assert_eq!(tree.target(file, ".leaf", &["value"]), "pkg/sub/leaf.py");
    assert_eq!(tree.target(file, ".", &["leaf"]), "pkg/sub/leaf.py");
    insta::assert_json_snapshot!(
        "an_import_inside_a_package_adds_nothing_for_its_own_package",
        tree.file_json(file)
    );
}

#[gsttt_scenario::scenario("python", "A FastAPI router included with a prefix")]
#[test]
fn a_fastapi_router_included_with_a_prefix() {
    let tree = index("fastapi_router");
    assert_eq!(
        tree.linked("app/api/bookings.py"),
        ["serves http GET /api/bookings/{}@6"]
    );
    // The router's own path is no route of the file: only the mounted one is served.
    assert_eq!(tree.contracts("app/api/bookings.py"), Vec::<String>::new());
    insta::assert_json_snapshot!(
        "a_fastapi_router_included_with_a_prefix",
        json!({ "bookings": tree.file_json("app/api/bookings.py"), "linked": tree.json["linked"] })
    );
}

#[gsttt_scenario::scenario("python", "A Flask blueprint serves under its prefix")]
#[test]
fn a_flask_blueprint_serves_under_its_prefix() {
    let tree = index("flask_blueprint");
    // No `register_blueprint` reaches the blueprint, so it is served under its own prefix.
    assert_eq!(tree.linked("app.py"), ["serves http DELETE /bookings/{}@6"]);
    assert_eq!(tree.contracts("app.py"), Vec::<String>::new());
    insta::assert_json_snapshot!(
        "a_flask_blueprint_serves_under_its_prefix",
        json!({ "app": tree.file_json("app.py"), "linked": tree.json["linked"] })
    );
}

#[test]
fn a_blueprint_registered_under_a_prefix_serves_under_it() {
    let tree = index("flask_register");
    assert_eq!(tree.contracts("app/bookings.py"), Vec::<String>::new());
    assert_eq!(
        tree.linked("app/bookings.py"),
        ["serves http GET /api/bookings/{}@6"]
    );
}

#[gsttt_scenario::scenario("python", "A FastAPI router mounted through another router")]
#[test]
fn a_fastapi_router_mounted_through_another_router() {
    let tree = index("fastapi_chain");
    // `app` adds `/api`, `sub` its own `/v1`, the mount in `sub` `/b`, and the router its own `/bookings`.
    assert_eq!(
        tree.linked("app/api/bookings.py"),
        ["serves http GET /api/v1/b/bookings/{}@6"]
    );
    assert_eq!(tree.contracts("app/api/bookings.py"), Vec::<String>::new());
    insta::assert_json_snapshot!(
        "a_fastapi_router_mounted_through_another_router",
        json!({ "bookings": tree.file_json("app/api/bookings.py"), "linked": tree.json["linked"] })
    );
}

#[test]
fn a_router_mounted_into_an_imported_router_keeps_the_chain() {
    let tree = index("fastapi_remote");
    // `sub` is made in `v1.py`, which declares no route and mounts nothing, and filled in `wiring.py`.
    assert_eq!(
        tree.linked("app/bookings.py"),
        ["serves http GET /api/v1/b/bookings/{}@6"]
    );
}

#[gsttt_scenario::scenario("python", "A router is served once for each way it is mounted")]
#[test]
fn a_router_is_served_once_for_each_way_it_is_mounted() {
    let tree = index("fastapi_mounts");
    // A mount with no prefix adds no second copy of the route.
    assert_eq!(tree.linked("app/health.py"), ["serves http GET /health@6"]);
    // Mounted plainly and under `/v2`: one copy per mount.
    assert_eq!(
        tree.linked("app/items.py"),
        [
            "serves http GET /items/{}@6",
            "serves http GET /v2/items/{}@6"
        ]
    );
    // A route on the application itself is not a router's route and stays in the file.
    assert_eq!(tree.contracts("app/main.py"), ["serves http GET /ping@11"]);
    assert_eq!(tree.linked("app/main.py"), Vec::<String>::new());
    for path in ["app/health.py", "app/items.py"] {
        assert_eq!(tree.contracts(path), Vec::<String>::new(), "{path}");
    }
    insta::assert_json_snapshot!(
        "a_router_is_served_once_for_each_way_it_is_mounted",
        json!({ "main": tree.file_json("app/main.py"), "linked": tree.json["linked"] })
    );
}

#[gsttt_scenario::scenario("python", "Routers that mount each other still serve their routes")]
#[test]
fn routers_that_mount_each_other_still_serve_their_routes() {
    let tree = index("fastapi_cycle");
    // `a` and `b` mount each other; the way up through the app is the only one that ends.
    assert_eq!(tree.linked("app/a.py"), ["serves http GET /a/x/one@9"]);
    assert_eq!(tree.linked("app/b.py"), ["serves http GET /a/x/b/y/two@9"]);
    insta::assert_json_snapshot!(
        "routers_that_mount_each_other_still_serve_their_routes",
        tree.json["linked"]
    );
}

#[gsttt_scenario::scenario("python", "A registration prefix replaces a blueprint's own")]
#[test]
fn a_registration_prefix_replaces_a_blueprints_own() {
    let tree = index("flask_replace");
    // `Blueprint(url_prefix="/bookings")` registered with `url_prefix="/api"` is served under `/api` alone.
    assert_eq!(
        tree.linked("app/bookings.py"),
        ["serves http GET /api/{}@6"]
    );
    // Registered with no prefix, the blueprint keeps its own.
    assert_eq!(
        tree.linked("app/reports.py"),
        ["serves http GET /reports/daily@6"]
    );
    for path in ["app/bookings.py", "app/reports.py"] {
        assert_eq!(tree.contracts(path), Vec::<String>::new(), "{path}");
    }
    insta::assert_json_snapshot!(
        "a_registration_prefix_replaces_a_blueprints_own",
        tree.json["linked"]
    );
}

#[gsttt_scenario::scenario("python", "Blueprints nest under their parent's prefix")]
#[test]
fn blueprints_nest_under_their_parents_prefix() {
    let tree = index("flask_nested");
    // The parent keeps its own `/admin` when it is registered plainly, and the child its own `/reports`.
    assert_eq!(
        tree.linked("app/reports.py"),
        ["serves http GET /admin/reports/daily@6"]
    );
    // `/store` replaces `/shop` and `/goods` replaces `/items`.
    assert_eq!(
        tree.linked("app/items.py"),
        ["serves http GET /store/goods/{}@6"]
    );
    insta::assert_json_snapshot!(
        "blueprints_nest_under_their_parents_prefix",
        tree.json["linked"]
    );
}

#[gsttt_scenario::scenario("python", "Django includes compose")]
#[test]
fn django_includes_compose() {
    let tree = index("django_includes");
    assert_eq!(
        tree.linked("bookings/urls.py"),
        ["serves http * /api/bookings/{}@6"]
    );
    // The module's own path is no route of the file: only the included one is served.
    assert_eq!(tree.contracts("bookings/urls.py"), Vec::<String>::new());
    // The including `path` is a mount and no route of its own.
    assert_eq!(tree.contracts("project/urls.py"), Vec::<String>::new());
    insta::assert_json_snapshot!(
        "django_includes_compose",
        json!({
            "project": tree.file_json("project/urls.py"),
            "bookings": tree.file_json("bookings/urls.py"),
            "linked": tree.json["linked"],
        })
    );
}

#[gsttt_scenario::scenario("python", "Requests become calls")]
#[test]
fn requests_become_calls() {
    let tree = index("requests_calls");
    assert_eq!(
        tree.contracts("client.py"),
        ["calls http POST /bookings/{}/cancel@8"]
    );
    let unresolved: Vec<u32> = tree
        .file("client.py")
        .facts
        .unresolved
        .iter()
        .map(|site| site.line)
        .collect();
    assert_eq!(unresolved, [12]);
    insta::assert_json_snapshot!("requests_become_calls", tree.file_json("client.py"));
}

#[gsttt_scenario::scenario("python", "A client with no base URL leaves a computed URL unread")]
#[test]
fn a_client_with_no_base_url_leaves_a_computed_url_unread() {
    let tree = index("client_urls");
    // A session or a client made without a base URL adds nothing to the call, so `f"{base}{path}"` holds no path.
    assert_eq!(tree.unread("client.py"), ["L7 session.get", "L11 ?.get"]);
    assert_eq!(
        tree.contracts("client.py"),
        [
            "calls http GET /v1/bookings@16",
            "calls http GET /bookings/1@21"
        ]
    );
    insta::assert_json_snapshot!(
        "a_client_with_no_base_url_leaves_a_computed_url_unread",
        tree.file_json("client.py")
    );
}

#[gsttt_scenario::scenario("python", "Model tables are declared")]
#[test]
fn model_tables_are_declared() {
    let tree = index("model_tables");
    assert_eq!(tree.contracts("models.py"), ["declares table bookings@9"]);
    assert_eq!(
        tree.contracts("items/models.py"),
        ["declares table booking_items@8"]
    );
    insta::assert_json_snapshot!(
        "model_tables_are_declared",
        json!({
            "sqlalchemy": tree.file_json("models.py"),
            "django": tree.file_json("items/models.py"),
        })
    );
}

#[gsttt_scenario::scenario("python", "A table name outside a model declares nothing")]
#[test]
fn a_table_name_outside_a_model_declares_nothing() {
    let tree = index("model_scopes");
    // Line 4 and 5 assign the names at module level, 16 and 17 in a method, 35 in a class `Meta` that sits in no class
    // and 40 in a class that is not called `Meta`.
    assert_eq!(
        tree.contracts("models.py"),
        [
            "declares table bookings@13",
            "declares table inner@22",
            "declares table deep@26",
            "declares table booking_items@31",
            "declares table audit_log@43",
            "declares table in_a_function@49",
        ]
    );
    insta::assert_json_snapshot!(
        "a_table_name_outside_a_model_declares_nothing",
        tree.file_json("models.py")
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A target of one segment matches wherever the name is assigned"
)]
#[test]
fn a_target_of_one_segment_matches_wherever_the_name_is_assigned() {
    let pack = r#"return {
        name = "anywhere",
        language = "python",
        assigns = { { target = "db_table", kind = "table", role = "declares", key = "$arg1" } },
    }"#;
    let packs = builtin_packs(&[("anywhere.lua", pack)]).unwrap();
    let tree = index_with("model_scopes", packs);
    assert_eq!(
        tree.contracts("models.py"),
        [
            "declares table module_variable@5",
            "declares table in_a_method_too@17",
            "declares table deep@26",
            "declares table booking_items@31",
            "declares table meta_alone@35",
            "declares table options_not_meta@40",
            "declares table audit_log@43",
            "declares table in_a_function@49",
        ]
    );
}

#[gsttt_scenario::scenario("python", "A migration declares its tables")]
#[test]
fn a_migration_declares_its_tables() {
    let tree = index("migration");
    let path = "migrations/versions/0001_bookings.py";
    assert_eq!(
        tree.contracts(path),
        [
            "declares table bookings@6",
            "declares table booking_items@7"
        ]
    );
    insta::assert_json_snapshot!("a_migration_declares_its_tables", tree.file_json(path));
}

#[gsttt_scenario::scenario("python", "A migration with an aliased op declares its tables")]
#[test]
fn a_migration_with_an_aliased_op_declares_its_tables() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"shop\"\ndependencies = [\"alembic\"]\n",
        ),
        (
            "versions/0002.py",
            r#"import sqlalchemy as sa
from alembic import op as o
from alembic.op import drop_table as drop


def upgrade():
    o.create_table("bookings", sa.Column("id", sa.Integer))
    o.create_index("ix_items_note", "booking_items", ["note"])
    drop("old_bookings")
"#,
        ),
    ]);
    assert_eq!(
        tree.contracts("versions/0002.py"),
        [
            "declares table bookings@7",
            "declares table booking_items@8",
            "declares table old_bookings@9"
        ]
    );
}

#[gsttt_scenario::scenario("python", "Raw SQL is embedded")]
#[test]
fn raw_sql_is_embedded() {
    let tree = index("raw_sql");
    let embedded = &tree.file("db.py").facts.embedded;
    assert_eq!(embedded.len(), 5, "{embedded:?}");
    assert_eq!(embedded[0].lang, "sql");
    assert_eq!(embedded[0].line, 7);
    assert_eq!(embedded[0].text, "SELECT id FROM bookings WHERE id = %s");
    insta::assert_json_snapshot!("raw_sql_is_embedded", tree.file_json("db.py"));
}

/// The line of `needle` in the file, counting the lines of the document from the one it starts on, which is how a
/// reader of the document places what it finds in it.
fn line_in(document: &Embedded, needle: &str) -> Line {
    assert!(
        document.text.contains(needle),
        "{needle:?} is not in {:?}",
        document.text
    );
    let before = document.text.split(needle).next().unwrap();
    document.line + Line::try_from(before.matches('\n').count()).unwrap()
}

#[gsttt_scenario::scenario("python", "A document starts on the line of its first literal")]
#[test]
fn a_document_starts_on_the_line_of_its_first_literal() {
    let tree = index("raw_sql");
    let embedded = &tree.file("db.py").facts.embedded;
    // `find_many` calls `execute(` on line 13 and opens its literal on line 14, as black lays a long call out.
    let document = &embedded[1];
    assert_eq!(document.line, 14, "{document:?}");
    assert_eq!(line_in(document, "SELECT id"), 15);
    assert_eq!(line_in(document, "FROM bookings"), 16);
    assert_eq!(line_in(document, "WHERE id IN %s"), 17);
}

#[gsttt_scenario::scenario("python", "Joined literals keep the lines of their parts")]
#[test]
fn joined_literals_keep_the_lines_of_their_parts() {
    let tree = index("raw_sql");
    let embedded = &tree.file("db.py").facts.embedded;
    // `find_joined` writes three literals one after the other, `find_added` joins the same three with `+`.
    for (document, first) in [(&embedded[2], 27), (&embedded[3], 38)] {
        assert_eq!(document.line, first, "{document:?}");
        assert_eq!(document.text, "SELECT id \nFROM bookings \nWHERE id = %s");
        assert_eq!(line_in(document, "FROM bookings"), first + 1);
        assert_eq!(line_in(document, "WHERE id = %s"), first + 2);
    }
}

#[gsttt_scenario::scenario("python", "A line break written as an escape adds no line")]
#[test]
fn a_line_break_written_as_an_escape_adds_no_line() {
    let tree = index("raw_sql");
    let embedded = &tree.file("db.py").facts.embedded;
    // `find_escaped` is one line of the file, with `\n` written out in its literal.
    let document = &embedded[4];
    assert_eq!(document.line, 48, "{document:?}");
    assert_eq!(document.text, "SELECT id FROM bookings WHERE id = %s");
    assert_eq!(line_in(document, "WHERE id = %s"), 48);
}

#[gsttt_scenario::scenario("python", "A line break in the source stays beside an escape")]
#[test]
fn a_line_break_in_the_source_stays_beside_an_escape() {
    // One escape in a literal over several lines must not take the lines of the source away: each `--` comment ends
    // where its line does.
    let tree = index_files(&[(
        "db.py",
        r#"def read(cursor):
    cursor.execute("""
        SELECT a -- note
        FROM t
        WHERE b = 'x\ny'
    """)
"#,
    )]);
    let embedded = &tree.file("db.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    let document = &embedded[0];
    assert_eq!(document.line, 2, "{document:?}");
    assert_eq!(line_in(document, "-- note"), 3, "{document:?}");
    assert_eq!(line_in(document, "FROM t"), 4, "{document:?}");
    assert_eq!(line_in(document, "WHERE b = 'x y'"), 5, "{document:?}");
}

#[gsttt_scenario::scenario("python", "A line continuation keeps the lines after it")]
#[test]
fn a_line_continuation_keeps_the_lines_after_it() {
    // The backslash joins two lines of the source into one of the string, and the line after them is still on line 5.
    let tree = index_files(&[(
        "db.py",
        r#"def read(cursor):
    cursor.execute("""
        SELECT a, \
               b
        FROM t
    """)
"#,
    )]);
    let embedded = &tree.file("db.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    let document = &embedded[0];
    assert_eq!(document.line, 2, "{document:?}");
    assert_eq!(line_in(document, "FROM t"), 5, "{document:?}");
}

#[gsttt_scenario::scenario(
    "python",
    "A comment before an escape-made line break is an unread site"
)]
#[test]
fn a_comment_before_an_escape_made_line_break_is_an_unread_site() {
    // The document cannot keep the break an escape makes, and without it the comment would run on into `FROM t`.
    let tree = index_files(&[(
        "db.py",
        r#"def read(cursor):
    cursor.execute("SELECT a -- note\nFROM t")
    cursor.execute("SELECT a\nFROM t -- note")
    cursor.execute("SELECT a -- x " "\nFROM t")
    cursor.execute("SELECT a -- x" + "\nFROM t")
"#,
    )]);
    assert_eq!(
        tree.unread("db.py"),
        [
            "L2 cursor.execute",
            "L4 cursor.execute",
            "L5 cursor.execute"
        ]
    );
    // A comment after the break is a comment of the last line, which the document keeps.
    let embedded = &tree.file("db.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    assert_eq!(embedded[0].line, 3);
    assert_eq!(embedded[0].text, "SELECT a FROM t -- note");
}

#[gsttt_scenario::scenario("python", "A SQL string built at run time is an unread site")]
#[test]
fn a_sql_string_built_at_run_time_is_an_unread_site() {
    let tree = index("sql_arguments");
    // An f-string, `+` and `%` on a literal, and `.format` on a text joined with `+`; the plain literal on line 6 is
    // read.
    assert_eq!(
        tree.unread("built.py"),
        [
            "L2 cursor.execute",
            "L3 cursor.execute",
            "L4 cursor.execute",
            "L5 cursor.execute"
        ]
    );
    let embedded = &tree.file("built.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    assert_eq!(embedded[0].line, 6);
    assert_eq!(embedded[0].text, "SELECT id FROM bookings");
    insta::assert_json_snapshot!(
        "a_sql_string_built_at_run_time_is_an_unread_site",
        tree.file_json("built.py")
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A format template with a replacement field is an unread site"
)]
#[test]
fn a_format_template_with_a_replacement_field_is_an_unread_site() {
    let tree = index_files(&[(
        "db.py",
        r#"def read(cursor, table, task):
    cursor.execute("SELECT id FROM {}".format(table))
    cursor.execute("SELECT id FROM {name} WHERE id = {0}".format(task, name=table))
    cursor.execute("SELECT id FROM {:{width}}".format(table, width=3))
    cursor.execute("SELECT '{{}}'".format())
"#,
    )]);
    assert_eq!(
        tree.unread("db.py"),
        [
            "L2 cursor.execute",
            "L3 cursor.execute",
            "L4 cursor.execute"
        ]
    );
    // Doubled braces are the braces they stand for, so the last call has nothing computed.
    let embedded = &tree.file("db.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    assert_eq!(embedded[0].line, 5);
    assert_eq!(embedded[0].text, "SELECT '{}'");
}

#[gsttt_scenario::scenario("python", "An argument that is not a string records nothing")]
#[test]
fn an_argument_that_is_not_a_string_records_nothing() {
    let tree = index("sql_arguments");
    // `pool.execute(task)` and `cursor.execute(queries[0])` are calls of methods that happen to be named `execute`.
    let facts = &tree.file("held.py").facts;
    assert!(facts.unresolved.is_empty(), "{:?}", facts.unresolved);
    assert!(facts.embedded.is_empty(), "{:?}", facts.embedded);
    // What a call to a function builds is not known to be SQL, so it is named instead.
    assert!(
        tree.blind_spots().contains(&"SQL built by a function call"),
        "{:?}",
        tree.blind_spots()
    );
}

#[test]
fn a_text_call_with_a_variable_is_still_an_unread_site() {
    let tree = index("sql_arguments");
    // `text(query)` can only hold a statement, so what it holds is unread; `execute(text(…))` holds a call and adds
    // none.
    assert_eq!(tree.unread("named.py"), ["L5 text"]);
    let embedded = &tree.file("named.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    assert_eq!(embedded[0].line, 6);
}

#[gsttt_scenario::scenario("python", "A receiver of more than three segments is not read")]
#[test]
fn a_receiver_of_more_than_three_segments_is_not_read() {
    let tree = index("sql_arguments");
    let facts = &tree.file("receivers.py").facts;
    let texts: Vec<_> = facts
        .embedded
        .iter()
        .map(|document| document.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "SELECT id FROM one",
            "SELECT id FROM two",
            "SELECT id FROM three"
        ]
    );
    assert!(facts.unresolved.is_empty(), "{:?}", facts.unresolved);
    let blind_spots = tree.blind_spots();
    assert!(
        blind_spots
            .contains(&"execute and executemany calls on a receiver of more than three segments"),
        "{blind_spots:?}"
    );
}

#[gsttt_scenario::scenario("python", "Every way of reading the environment is seen")]
#[test]
fn every_way_of_reading_the_environment_is_seen() {
    let tree = index("environment");
    assert_eq!(
        tree.contracts("config.py"),
        [
            "calls env A@5",
            "calls env B@6",
            "calls env C@7",
            "calls env DATABASE_URL@11",
        ]
    );
    insta::assert_json_snapshot!(
        "every_way_of_reading_the_environment_is_seen",
        tree.file_json("config.py")
    );
}

#[gsttt_scenario::scenario("python", "A settings prefix is applied")]
#[test]
fn a_settings_prefix_is_applied() {
    let tree = index("settings_prefix");
    assert_eq!(tree.contracts("settings.py"), ["calls env APP_PORT@7"]);
    insta::assert_json_snapshot!(
        "a_settings_prefix_is_applied",
        tree.file_json("settings.py")
    );
}

#[gsttt_scenario::scenario("python", "A settings field with an alias is read under the alias")]
#[test]
fn a_settings_field_with_an_alias_is_read_under_the_alias() {
    let tree = index("settings_alias");
    // `validation_alias` wins over `alias`, an alias is not prefixed, and a field whose alias is not a literal is an
    // unread site rather than a variable named after the field.
    assert_eq!(
        tree.contracts("settings.py"),
        [
            "calls env DB_URL@10",
            "calls env SERVICE_TOKEN@11",
            "calls env REGION@12",
            "calls env APP_DEBUG@15",
            "calls env APP_TIMEOUT@16",
        ]
    );
    assert_eq!(tree.unresolved_lines("settings.py"), [13, 14]);
    insta::assert_json_snapshot!(
        "a_settings_field_with_an_alias_is_read_under_the_alias",
        tree.file_json("settings.py")
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A settings field with a pydantic 1 env name is read under it"
)]
#[test]
fn a_settings_field_with_a_pydantic_1_env_name_is_read_under_it() {
    let tree = index_files(&[(
        "settings.py",
        r#"from pydantic import BaseSettings, Field
from pydantic_settings import SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="APP_")
    database_url: str = Field(env="DB_URL")
    region: str = Field("eu", env="REGION", alias="OLD_REGION")
    timeout: int = 30
"#,
    )]);
    assert_eq!(
        tree.contracts("settings.py"),
        [
            "calls env DB_URL@7",
            "calls env REGION@8",
            "calls env APP_TIMEOUT@9"
        ]
    );
    assert_eq!(tree.unread("settings.py"), Vec::<String>::new());
}

#[gsttt_scenario::scenario("python", "A None alias is no alias")]
#[test]
fn a_none_alias_is_no_alias() {
    let tree = index_files(&[(
        "settings.py",
        r#"from pydantic import BaseSettings, Field


class Settings(BaseSettings):
    port: int = Field(8000, alias=None)
    mode: str = Field("dev", validation_alias=None, alias="MODE_NAME")
"#,
    )]);
    assert_eq!(
        tree.contracts("settings.py"),
        ["calls env PORT@5", "calls env MODE_NAME@6"]
    );
    assert_eq!(tree.unread("settings.py"), Vec::<String>::new());
}

#[gsttt_scenario::scenario("python", "Setting a default for an environment variable reads it")]
#[test]
fn setting_a_default_for_an_environment_variable_reads_it() {
    let tree = index("environment");
    assert_eq!(
        tree.contracts("defaults.py"),
        ["calls env DJANGO_SETTINGS_MODULE@4", "calls env PORT@5",]
    );
    assert_eq!(tree.unresolved_lines("defaults.py"), [6]);
}

#[gsttt_scenario::scenario("python", "The fields of a settings subclass are a blind spot")]
#[test]
fn the_fields_of_a_settings_subclass_are_a_blind_spot() {
    let tree = index("settings_subclass");
    assert_eq!(tree.contracts("settings.py"), ["calls env LOG_LEVEL@5"]);
    let blind_spots = tree.blind_spots();
    assert!(
        blind_spots
            .contains(&"fields of a settings class that derives from another settings class"),
        "{blind_spots:?}"
    );
}

#[gsttt_scenario::scenario("python", "A module imported under another name is followed")]
#[test]
fn a_module_imported_under_another_name_is_followed() {
    let tree = index("aliases");
    // `import requests as r` makes `r.post` the call `requests.post`.
    assert_eq!(
        tree.contracts("requests_module.py"),
        ["calls http POST /bookings/{}/cancel@7"]
    );
    insta::assert_json_snapshot!(
        "a_module_imported_under_another_name_is_followed",
        tree.file_json("requests_module.py")
    );
}

#[gsttt_scenario::scenario("python", "A name imported from a module is followed")]
#[test]
fn a_name_imported_from_a_module_is_followed() {
    let tree = index("aliases");
    // `from requests import post as send` calls `post`: the verb is the one of the function, not of the name it is
    // imported as.
    assert_eq!(
        tree.contracts("requests_name.py"),
        [
            "calls http GET /bookings/{}@8",
            "calls http POST /bookings/{}/cancel@12"
        ]
    );
    insta::assert_json_snapshot!(
        "a_name_imported_from_a_module_is_followed",
        tree.file_json("requests_name.py")
    );
}

#[gsttt_scenario::scenario("python", "A client made through an aliased module is followed")]
#[test]
fn a_client_made_through_an_aliased_module_is_followed() {
    let tree = index("aliases");
    // `hx.Client(…)` is an `httpx.Client`, so the calls on its variable are read behind its base URL.
    assert_eq!(
        tree.contracts("httpx_module.py"),
        [
            "calls http GET /bookings/{}@7",
            "calls http GET /v1/bookings@12"
        ]
    );
    insta::assert_json_snapshot!(
        "a_client_made_through_an_aliased_module_is_followed",
        tree.file_json("httpx_module.py")
    );
}

#[gsttt_scenario::scenario("python", "A function imported under another name embeds its document")]
#[test]
fn a_function_imported_under_another_name_embeds_its_document() {
    let tree = index("aliases");
    let embedded = &tree.file("sqlalchemy_name.py").facts.embedded;
    assert_eq!(embedded.len(), 1, "{embedded:?}");
    assert_eq!(embedded[0].lang, "sql");
    assert_eq!(embedded[0].line, 5);
    assert_eq!(embedded[0].text, "SELECT id FROM bookings WHERE id = :id");
    insta::assert_json_snapshot!(
        "a_function_imported_under_another_name_embeds_its_document",
        tree.file_json("sqlalchemy_name.py")
    );
}

#[gsttt_scenario::scenario(
    "python",
    "A router class imported under another name serves its routes"
)]
#[test]
fn a_router_class_imported_under_another_name_serves_its_routes() {
    let tree = index("aliases");
    // `app = fa.FastAPI()` is an application and its route a fact of the file; `router = R(prefix=…)` is a router,
    // which `link` serves behind its prefix.
    assert_eq!(
        tree.contracts("fastapi_router.py"),
        ["serves http GET /ping@8"]
    );
    assert_eq!(
        tree.linked("fastapi_router.py"),
        ["serves http GET /bookings/{}@13"]
    );
    insta::assert_json_snapshot!(
        "a_router_class_imported_under_another_name_serves_its_routes",
        json!({
            "file": tree.file_json("fastapi_router.py"),
            "linked": tree.linked("fastapi_router.py"),
        })
    );
}

#[gsttt_scenario::scenario("python", "The environment is read through an aliased os")]
#[test]
fn the_environment_is_read_through_an_aliased_os() {
    let tree = index("aliases");
    assert_eq!(
        tree.contracts("environment.py"),
        ["calls env A@3", "calls env B@4", "calls env C@5"]
    );
    insta::assert_json_snapshot!(
        "the_environment_is_read_through_an_aliased_os",
        tree.file_json("environment.py")
    );
}

#[gsttt_scenario::scenario("python", "Django routes imported under another name compose")]
#[test]
fn django_routes_imported_under_another_name_compose() {
    let tree = index("aliases");
    assert_eq!(
        tree.linked("bookings/urls.py"),
        ["serves http * /api/bookings/{}@6"]
    );
    assert_eq!(tree.contracts("bookings/urls.py"), Vec::<String>::new());
    assert_eq!(tree.contracts("project/urls.py"), Vec::<String>::new());
    insta::assert_json_snapshot!(
        "django_routes_imported_under_another_name_compose",
        json!({
            "project": tree.file_json("project/urls.py"),
            "bookings": tree.file_json("bookings/urls.py"),
            "linked": tree.linked("bookings/urls.py"),
        })
    );
}

#[gsttt_scenario::scenario("python", "A name imported twice is listed once")]
#[test]
fn a_name_imported_twice_is_listed_once() {
    let tree = index("aliases");
    // `from bookings import views as first, views as second` imports `views` twice and the import names it once.
    assert_eq!(
        tree.target("reexport.py", "bookings", &["views"]),
        "bookings/views.py"
    );
    insta::assert_json_snapshot!(
        "a_name_imported_twice_is_listed_once",
        tree.file_json("reexport.py")
    );
}

#[gsttt_scenario::scenario("python", "A setup.py with bracketed requirements is read")]
#[test]
fn a_setup_py_with_bracketed_requirements_is_read() {
    let tree = index("setup_py");
    // `fastapi[standard]` is what switches the FastAPI pack on, and a comment with an apostrophe follows it. The
    // route is on a router nothing includes, which `link` serves behind the router's own prefix.
    assert_eq!(
        tree.linked("lib/bookings/api.py"),
        ["serves http GET /ping@6"]
    );
    // `package_dir={"": "lib"}` is what makes `lib` a source root.
    assert_eq!(
        tree.target("app.py", "bookings", &["api"]),
        "lib/bookings/api.py"
    );
    insta::assert_json_snapshot!(
        "a_setup_py_with_bracketed_requirements_is_read",
        json!({
            "file": tree.file_json("lib/bookings/api.py"),
            "linked": tree.linked("lib/bookings/api.py"),
        })
    );
}

#[gsttt_scenario::scenario("python", "Packs follow the manifest")]
#[test]
fn packs_follow_the_manifest() {
    let tree = index("packs_follow_manifest");
    // The Flask route is a site; the FastAPI decorator, whose pack the manifest does not switch on, is not.
    assert_eq!(tree.contracts("app.py"), ["serves http GET /flask@8"]);
    let via: Vec<&str> = tree.output.projects[0]
        .provides
        .iter()
        .flat_map(|provide| provide.via.iter().map(String::as_str))
        .collect();
    assert!(via.contains(&"flask"), "{via:?}");
    assert!(!via.contains(&"fastapi"), "{via:?}");
    insta::assert_json_snapshot!("packs_follow_the_manifest", tree.file_json("app.py"));
}

/// The salt of each project of the tree, by its root.
fn salts(tree: &Tree) -> HashMap<&str, u64> {
    tree.output
        .projects
        .iter()
        .map(|project| (project.root.as_str(), project.salt))
        .collect()
}

#[test]
fn the_salt_of_a_project_follows_the_packs_its_manifest_switches_on() {
    // `a` and `c` depend on FastAPI and `b` on Flask, so a file of `b` is not read the way the same file of `a` is.
    let tree = index("pack_selection");
    let salts = salts(&tree);
    assert_eq!(salts["a"], salts["c"], "{salts:?}");
    assert_ne!(salts["a"], salts["b"], "{salts:?}");
}

#[test]
fn two_packs_of_one_name_are_told_apart_by_the_salt() {
    let pack = |detect: &str| -> Pack {
        serde_json::from_value(
            json!({ "name": "shared", "language": "python", "detect": [detect] }),
        )
        .unwrap()
    };
    let tree = index_with("pack_selection", vec![pack("fastapi"), pack("flask")]);
    let salts = salts(&tree);
    assert_eq!(salts["a"], salts["c"], "{salts:?}");
    assert_ne!(salts["a"], salts["b"], "{salts:?}");
}

#[gsttt_scenario::scenario("python", "Blind spots are reported")]
#[test]
fn blind_spots_are_reported() {
    let tree = index("blind_spots");
    let blind_spots = tree.blind_spots();
    for words in [
        "Flask routes with several methods beyond the first",
        "Flask routes whose methods are held in a variable, which are read as GET",
        "prefixes held in variables",
        "Django default table names",
        "execute and executemany calls on a receiver of more than three segments",
        "SQL held in a variable",
        "fields of a settings class that derives from another settings class",
        "an alias written inside a field's annotation",
    ] {
        assert!(blind_spots.contains(&words), "{words}: {blind_spots:?}");
    }
    insta::assert_json_snapshot!("blind_spots_are_reported", tree.json["projects"]);
}

#[gsttt_scenario::scenario("python", "A table name given by a declared_attr is a blind spot")]
#[test]
fn a_table_name_given_by_a_declared_attr_is_a_blind_spot() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"shop\"\ndependencies = [\"sqlalchemy\"]\n",
        ),
        (
            "models.py",
            r"from sqlalchemy.orm import declarative_base, declared_attr

Base = declarative_base()


class TableNameMixin:
    @declared_attr
    def __tablename__(cls):
        return cls.__name__.lower()


class Booking(TableNameMixin, Base):
    id = 1
",
        ),
    ]);
    assert_eq!(tree.contracts("models.py"), Vec::<String>::new());
    let words = "table names given by a declared_attr or inherited from a mixin";
    assert!(
        tree.blind_spots().contains(&words),
        "{:?}",
        tree.blind_spots()
    );
}

#[gsttt_scenario::scenario("python", "A function from a star import is not followed")]
#[test]
fn a_function_from_a_star_import_is_not_followed() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"shop\"\ndependencies = [\"requests\"]\n",
        ),
        (
            "client.py",
            "from requests import *\n\n\ndef cancel(id):\n    get(f\"{BASE}/bookings/{id}\")\n",
        ),
    ]);
    assert_eq!(tree.contracts("client.py"), Vec::<String>::new());
    let words =
        "names brought in by a star import, which are not matched to the module they come from";
    let calls = tree.output.projects[0]
        .provides
        .iter()
        .find(|provide| provide.kind == ContractKind::Http && provide.role == Role::Calls)
        .expect("the requests pack gives the project http calls");
    assert!(
        calls.blind_spots.iter().any(|spot| spot == words),
        "{:?}",
        calls.blind_spots
    );
}

#[test]
fn a_repository_without_a_manifest_gets_a_root_project() {
    let tree = index("raw_sql");
    assert_eq!(tree.output.projects.len(), 1);
    assert_eq!(tree.file_json("db.py")["project"], "python .");
}

#[test]
fn the_built_in_packs_are_python_packs_named_after_their_files() {
    let packs = builtin_packs(PACKS).unwrap();
    assert_eq!(packs.len(), PACKS.len());
    for (pack, (file, _)) in packs.iter().zip(PACKS) {
        assert_eq!(pack.language, "python");
        assert_eq!(format!("{}.lua", pack.name), *file);
    }
}

// Review fixes, round 3. Each test fails on the code the second round left and passes on its fix.

const API_DECLARES_COMMON: &str =
    "[project]\nname = \"api\"\ndependencies = [\"requests\", \"common\"]\n";

#[gsttt_scenario::scenario(
    "python",
    "A project of the repository is searched only when it is declared"
)]
#[test]
fn a_project_of_the_repository_is_searched_only_when_it_is_declared() {
    let tree = index_files(&[
        ("pyproject.toml", "[project]\nname = \"monorepo\"\n"),
        ("scripts/__init__.py", ""),
        ("services/api/pyproject.toml", API_DECLARES_COMMON),
        (
            "services/api/app.py",
            "import requests\nimport common.util\nimport helper\nimport scripts\n",
        ),
        (
            "libs/common/pyproject.toml",
            "[project]\nname = \"common\"\n",
        ),
        ("libs/common/common/__init__.py", ""),
        ("libs/common/common/util.py", ""),
        ("tools/pyproject.toml", "[project]\nname = \"tools\"\n"),
        ("tools/requests/__init__.py", ""),
        ("tools/helper/__init__.py", ""),
    ]);
    let target = |spec: &str| tree.target("services/api/app.py", spec, &[]);
    // `requests` is the declared distribution of PyPI, whatever a project nobody declared holds under that name.
    assert_eq!(target("requests"), "package requests");
    assert_eq!(
        target("common.util"),
        "libs/common/common/__init__.py + libs/common/common/util.py"
    );
    // Neither `tools` nor the project at the root is declared, so what they hold is no module of `services/api`.
    assert_eq!(target("helper"), "unresolved UnknownPackage");
    assert_eq!(target("scripts"), "unresolved UnknownPackage");
}

#[test]
fn declared_projects_are_searched_in_the_order_of_their_paths() {
    let tree = index_files(&[
        (
            "app/pyproject.toml",
            "[project]\nname = \"app\"\ndependencies = [\"zeta\", \"alpha\"]\n",
        ),
        ("app/main.py", "import shared\n"),
        ("libs/alpha/pyproject.toml", "[project]\nname = \"alpha\"\n"),
        ("libs/alpha/shared.py", ""),
        ("libs/zeta/pyproject.toml", "[project]\nname = \"zeta\"\n"),
        ("libs/zeta/shared.py", ""),
    ]);
    assert_eq!(
        tree.target("app/main.py", "shared", &[]),
        "libs/alpha/shared.py"
    );
}

#[gsttt_scenario::scenario("python", "A regular package hides the same name further along")]
#[test]
fn a_regular_package_hides_the_same_name_further_along() {
    let tree = index_files(&[
        (
            "a/pyproject.toml",
            "[project]\nname = \"a\"\ndependencies = [\"b\"]\n",
        ),
        ("a/util/__init__.py", ""),
        ("a/main.py", "import util.two\nimport util\n"),
        ("b/pyproject.toml", "[project]\nname = \"b\"\n"),
        ("b/util/__init__.py", ""),
        ("b/util/two.py", ""),
        ("c/pyproject.toml", "[project]\nname = \"c\"\n"),
        ("c/src/pkg/__init__.py", ""),
        ("c/pkg/more.py", ""),
        ("c/logging/__init__.py", ""),
        ("c/main.py", "import pkg.more\nimport logging.handlers\n"),
    ]);
    // `a/util` is the first `util` on the path and has no `two`, so the `util` of `b` supplies nothing.
    assert_eq!(
        tree.target("a/main.py", "util.two", &[]),
        "unresolved NotFound"
    );
    assert_eq!(tree.target("a/main.py", "util", &[]), "a/util/__init__.py");
    // Between the roots of one project the same holds: `src/pkg` comes before the directory `pkg` beside it.
    assert_eq!(
        tree.target("c/main.py", "pkg.more", &[]),
        "unresolved NotFound"
    );
    // The standard library is looked in before the later roots, as the order says, so it still has its own module.
    assert_eq!(tree.target("c/main.py", "logging.handlers", &[]), "builtin");
}

const BOOKINGS_ROUTER: &str = "from fastapi import APIRouter\n\nrouter = APIRouter(prefix=\"/bookings\")\n\n\n@router.get(\"/{booking_id}\")\ndef get_booking(booking_id: int):\n    return {}\n";

const FASTAPI_PROJECT: &str =
    "[project]\nname = \"bookings\"\ndependencies = [\"fastapi>=0.110\"]\n";

#[gsttt_scenario::scenario("python", "A router mounted through a re-export")]
#[test]
fn a_router_mounted_through_a_reexport() {
    let tree = index_files(&[
        ("pyproject.toml", FASTAPI_PROJECT),
        ("app/__init__.py", ""),
        (
            "app/api/__init__.py",
            "from .bookings import router as bookings_router\n",
        ),
        ("app/api/bookings.py", BOOKINGS_ROUTER),
        (
            "app/main.py",
            "from fastapi import FastAPI\n\nfrom app.api import bookings_router\n\napp = FastAPI()\napp.include_router(bookings_router, prefix=\"/api\")\n",
        ),
    ]);
    // The name `main.py` mounts is the one the package gives the router of `bookings`, not a router of the package.
    assert_eq!(
        tree.linked("app/api/bookings.py"),
        ["serves http GET /api/bookings/{}@6"]
    );
    assert_eq!(tree.contracts("app/api/bookings.py"), Vec::<String>::new());
}

#[test]
fn a_router_reexported_by_two_packages_is_found_through_both() {
    let tree = index_files(&[
        ("pyproject.toml", FASTAPI_PROJECT),
        (
            "app/__init__.py",
            "from .api import bookings_router as exported\n",
        ),
        (
            "app/api/__init__.py",
            "from .bookings import router as bookings_router\n",
        ),
        ("app/api/bookings.py", BOOKINGS_ROUTER),
        (
            "app/main.py",
            "from fastapi import FastAPI\n\nfrom app import exported\n\napp = FastAPI()\napp.include_router(exported, prefix=\"/v2\")\n",
        ),
    ]);
    assert_eq!(
        tree.linked("app/api/bookings.py"),
        ["serves http GET /v2/bookings/{}@6"]
    );
}

#[gsttt_scenario::scenario("python", "A router whose every chain is cut serves nothing")]
#[test]
fn a_router_whose_every_chain_is_cut_serves_nothing() {
    let router = |own: &str, other: &str, prefix: &str, path: &str| {
        format!(
            "from fastapi import APIRouter\n\nfrom app import {other}\n\nrouter = APIRouter(prefix=\"{own}\")\nrouter.include_router({other}.router, prefix=\"{prefix}\")\n\n\n@router.get(\"{path}\")\ndef handler():\n    return {{}}\n"
        )
    };
    let tree = index_files(&[
        ("pyproject.toml", FASTAPI_PROJECT),
        ("app/__init__.py", ""),
        ("app/a.py", &router("/x", "b", "/b", "/one")),
        ("app/b.py", &router("/y", "a", "/a", "/two")),
    ]);
    // Each router is included by the other and by nothing else, so no chain ends at a router nobody includes: a path
    // at the router's own prefix would be one that nothing serves.
    assert_eq!(tree.linked("app/a.py"), Vec::<String>::new());
    assert_eq!(tree.linked("app/b.py"), Vec::<String>::new());
}

#[gsttt_scenario::scenario("python", "A prefix that is not a literal cuts the chain")]
#[test]
fn a_prefix_that_is_not_a_literal_cuts_the_chain() {
    let tree = index_files(&[
        ("pyproject.toml", FASTAPI_PROJECT),
        ("app/__init__.py", ""),
        (
            "app/items.py",
            "from fastapi import APIRouter\n\nrouter = APIRouter(prefix=\"/items\")\n\n\n@router.get(\"/{item_id}\")\ndef get_item(item_id: int):\n    return {}\n",
        ),
        (
            "app/main.py",
            "from fastapi import FastAPI\n\nfrom app import items\n\nPREFIX = \"/api\"\napp = FastAPI()\napp.include_router(items.router, prefix=PREFIX)\napp.include_router(items.router, prefix=\"/v2\")\n",
        ),
    ]);
    // The mount with the prefix held in a variable serves the router at a path that is not known, so only the other
    // mount is followed; the call is an unread site of the file that makes it.
    assert_eq!(
        tree.linked("app/items.py"),
        ["serves http GET /v2/items/{}@6"]
    );
    assert_eq!(tree.unread("app/main.py"), ["L7 app.include_router"]);
}

#[test]
fn a_registration_or_an_include_with_a_prefix_held_in_a_variable_is_cut_too() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"web\"\ndependencies = [\"flask\", \"django\"]\n",
        ),
        (
            "shop/__init__.py",
            "from flask import Flask\n\nfrom shop.items import bp\n\nURL = \"/api\"\napp = Flask(__name__)\napp.register_blueprint(bp, url_prefix=URL)\n",
        ),
        (
            "shop/items.py",
            "from flask import Blueprint\n\nbp = Blueprint(\"items\", __name__, url_prefix=\"/items\")\n\n\n@bp.get(\"/<int:id>\")\ndef get_item(id):\n    return {}\n",
        ),
        (
            "project/urls.py",
            "from django.urls import include, path\n\nPREFIX = \"api/\"\nurlpatterns = [\n    path(PREFIX, include(\"bookings.urls\")),\n    path(\"v2/\", include(\"bookings.urls\")),\n]\n",
        ),
        (
            "bookings/urls.py",
            "from django.urls import path\n\nfrom . import views\n\nurlpatterns = [\n    path(\"bookings/<int:pk>/\", views.detail),\n]\n",
        ),
    ]);
    assert_eq!(tree.linked("shop/items.py"), Vec::<String>::new());
    assert_eq!(
        tree.linked("bookings/urls.py"),
        ["serves http * /v2/bookings/{}@6"]
    );
}

#[gsttt_scenario::scenario("python", "The mounts that are not followed are named")]
#[test]
fn the_mounts_that_are_not_followed_are_named() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"web\"\ndependencies = [\"fastapi\", \"flask\", \"django\"]\n",
        ),
        ("app.py", "import fastapi\n"),
    ]);
    let blind_spots = tree.blind_spots();
    let mounts = blind_spots
        .iter()
        .find(|words| words.starts_with("mounts of a router"))
        .expect("the provides name the mounts that are not followed");
    // Every way a chain of mounts is cut is in that one line, because `link` cannot report a site it did not read.
    for cut in [
        "another Python project",
        "a cycle",
        "deeper than 32 routers",
        "more than 64 ways",
        "a prefix held in a variable",
        "an import that cannot be resolved",
        "re-exports",
    ] {
        assert!(mounts.contains(cut), "{cut}: {mounts}");
    }
    assert!(
        blind_spots.contains(&"applications mounted under a path with mount, as Starlette does"),
        "{blind_spots:?}"
    );
}

#[test]
fn a_router_included_with_a_prefix_and_without_is_served_at_both() {
    let tree = index_files(&[
        ("pyproject.toml", FASTAPI_PROJECT),
        ("app/__init__.py", ""),
        (
            "app/bookings.py",
            "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n\n@router.get(\"/bookings/{booking_id}\")\ndef get_booking(booking_id: int):\n    return {}\n",
        ),
        (
            "app/main.py",
            "from fastapi import FastAPI\n\nfrom app import bookings\n\napp = FastAPI()\napp.include_router(bookings.router, prefix=\"/api\")\napp.include_router(bookings.router)\n",
        ),
    ]);
    // The mount with no prefix is a mount like the other: the router is served at the root as well, and the file keeps
    // no copy of the route of its own.
    assert_eq!(
        tree.linked("app/bookings.py"),
        [
            "serves http GET /api/bookings/{}@6",
            "serves http GET /bookings/{}@6"
        ]
    );
    assert_eq!(tree.contracts("app/bookings.py"), Vec::<String>::new());
}

#[gsttt_scenario::scenario("python", "An attribute assignment declares no table")]
#[test]
fn an_attribute_assignment_declares_no_table() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"web\"\ndependencies = [\"sqlalchemy\", \"django\"]\n",
        ),
        (
            "models.py",
            "from sqlalchemy.orm import DeclarativeBase\n\n\nclass Base(DeclarativeBase):\n    pass\n\n\nclass Booking(Base):\n    __tablename__ = \"bookings\"\n\n    def rename(self):\n        self.__tablename__ = \"scratch\"\n\n\nBooking.__tablename__ = \"v2\"\nBooking.Meta.db_table = \"x\"\n",
        ),
    ]);
    // A name written with dots is an attribute of an object, which the qualified targets of the packs, `*.__tablename__`
    // and `*.Meta.db_table`, are not about: they name the classes around a plain assignment.
    assert_eq!(tree.contracts("models.py"), ["declares table bookings@9"]);
}

#[gsttt_scenario::scenario(
    "python",
    "A settings prefix that is not a literal leaves its fields unread"
)]
#[test]
fn a_settings_prefix_that_is_not_a_literal_leaves_its_fields_unread() {
    let tree = index_files(&[(
        "settings.py",
        "from pydantic_settings import BaseSettings, SettingsConfigDict\n\nPREFIX = \"APP_\"\n\n\nclass Call(BaseSettings):\n    model_config = SettingsConfigDict(env_prefix=PREFIX)\n\n    port: int\n    token: str = Field(alias=\"API_TOKEN\")\n\n\nclass Table(BaseSettings):\n    model_config = {\"env_prefix\": f\"{PREFIX}X_\"}\n\n    host: str\n\n\nclass Inner(BaseSettings):\n    class Config:\n        env_prefix = PREFIX\n\n    region: str\n\n\nclass Literal(BaseSettings):\n    model_config = SettingsConfigDict(env_prefix=\"LIT_\")\n\n    mode: str\n",
    )]);
    // The prefix is part of the name of each field that has no alias, so a field cannot be named when the prefix is
    // computed; an alias is never prefixed and stays read.
    assert_eq!(
        tree.contracts("settings.py"),
        ["calls env API_TOKEN@10", "calls env LIT_MODE@29"]
    );
    assert_eq!(
        tree.unread("settings.py"),
        ["L9 port", "L16 host", "L23 region"]
    );
}

#[gsttt_scenario::scenario("python", "A function imported by name is no method of a receiver")]
#[test]
fn a_function_imported_by_name_is_no_method_of_a_receiver() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"web\"\ndependencies = [\"httpx\", \"pydantic-settings\"]\n",
        ),
        (
            "app.py",
            "import sqlite3\nfrom mylib import execute, get\nfrom pydantic_settings import BaseSettings as Base\n\n\nclass Settings(Base):\n    port: int\n\n\ndef run(table):\n    execute(\"rm -rf /tmp/x\")\n    execute(f\"SELECT * FROM {table}\")\n    get(\"https://example.com/x\")\n    cursor = sqlite3.connect(\"db\").cursor()\n    cursor.execute(\"SELECT 1 FROM bookings\")\n",
        ),
    ]);
    // `mylib.execute` and `mylib.get` match `*.execute` and `*.get` only when the module a name was imported from is
    // taken for the receiver, which it is not. A pack that wants a library's own spelling names the library.
    let file = tree.file("app.py");
    let embedded: Vec<_> = file
        .facts
        .embedded
        .iter()
        .map(|document| (document.line, document.text.as_str()))
        .collect();
    assert_eq!(embedded, [(15, "SELECT 1 FROM bookings")]);
    assert_eq!(tree.unread("app.py"), Vec::<String>::new());
    assert_eq!(tree.contracts("app.py"), ["calls env PORT@7"]);
}

#[gsttt_scenario::scenario("python", "A name bound by two imports is read as the last")]
#[test]
fn a_name_bound_by_two_imports_is_read_as_the_last() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"web\"\ndependencies = [\"requests\", \"httpx\"]\n",
        ),
        (
            "clients.py",
            "def first():\n    import requests as c\n    c.get(\"/x\")\n\n\ndef second():\n    import httpx as c\n    c.get(\"/y\")\n\n\ntry:\n    import httpx as http\nexcept ImportError:\n    import requests as http\n\nhttp.get(\"/z\")\n",
        ),
    ]);
    // The binding in force is the last one the walk met, so each call goes with the library imported just above it.
    let facts: Vec<_> = tree
        .file("clients.py")
        .facts
        .contracts
        .iter()
        .map(|fact| (fact.line, fact.via.as_str()))
        .collect();
    assert_eq!(facts, [(3, "requests"), (8, "httpx"), (16, "requests")]);
    // The fallback library is the one that is read, which the provides of the clients say.
    assert!(
        tree.blind_spots().contains(
            &"a client library imported under one name in try and in except, of which the last import is read"
        ),
        "{:?}",
        tree.blind_spots()
    );
}

#[gsttt_scenario::scenario("python", "A setup.py keyword is read only when a literal follows it")]
#[test]
fn a_setup_py_keyword_is_read_only_when_a_literal_follows_it() {
    let router = "from fastapi import APIRouter\n\nrouter = APIRouter()\n\n\n@router.get(\"/ping\")\ndef ping():\n    return {}\n";
    // `install_requires` is a name, and the list after it belongs to `extras_require`, which the reader does not take.
    let extras = index_files(&[
        (
            "setup.py",
            "from setuptools import setup\n\nsetup(\n    install_requires=REQS,\n    extras_require={\"dev\": [\"fastapi\"]},\n)\n",
        ),
        ("app.py", router),
    ]);
    assert_eq!(extras.linked("app.py"), Vec::<String>::new());
    // `package_dir` is a name, and the dict after it is the one of `entry_points`.
    let dirs = index_files(&[
        (
            "setup.py",
            "from setuptools import setup\n\nsetup(\n    package_dir=PKG_DIRS,\n    entry_points={\"\": \"lib\"},\n)\n",
        ),
        ("lib/bookings/__init__.py", ""),
        ("lib/bookings/api.py", ""),
        ("app.py", "from bookings import api\n"),
    ]);
    assert_eq!(
        dirs.target("app.py", "bookings", &["api"]),
        "unresolved UnknownPackage"
    );
    // A backslash makes the quote after it part of the string, so the requirement after that string is still read.
    let escaped = index_files(&[
        (
            "setup.py",
            "from setuptools import setup\n\nsetup(\n    install_requires=[\"left\\\"right\", \"fastapi\"],\n)\n",
        ),
        ("app.py", router),
    ]);
    assert_eq!(escaped.linked("app.py"), ["serves http GET /ping@6"]);
}

#[gsttt_scenario::scenario("python", "A stub without its source loses to a declared distribution")]
#[test]
fn a_stub_without_its_source_loses_to_a_declared_distribution() {
    let tree = index_files(&[
        (
            "pyproject.toml",
            "[project]\nname = \"app\"\ndependencies = [\"foo\", \"bar\", \"real\", \"mixed\"]\n",
        ),
        ("foo.pyi", "def f() -> int: ...\n"),
        ("bar/__init__.pyi", ""),
        ("bar/sub.pyi", ""),
        ("baz.pyi", "def g() -> int: ...\n"),
        ("real.py", ""),
        ("real.pyi", ""),
        ("mixed/__init__.py", ""),
        ("mixed/only.pyi", ""),
        (
            "app.py",
            "import foo\nimport bar.sub\nimport baz\nimport real\nimport mixed.only\n",
        ),
    ]);
    // Nothing runs from a stub, so the library the project declares is what these two imports load.
    assert_eq!(tree.target("app.py", "foo", &[]), "package foo");
    assert_eq!(tree.target("app.py", "bar.sub", &[]), "package bar");
    // A stub nobody declares a library for is all there is to follow.
    assert_eq!(tree.target("app.py", "baz", &[]), "baz.pyi");
    // A stub beside its source, and a package of source files with a stub in it, are the project's own.
    assert_eq!(tree.target("app.py", "real", &[]), "real.py + real.pyi");
    assert_eq!(
        tree.target("app.py", "mixed.only", &[]),
        "mixed/__init__.py + mixed/only.pyi"
    );
}

/// A file with seven layers of two routers, each router included in both routers of the layer above, so that `a0`
/// is reached in 128 ways. The `include_router` calls come from the lowest layer up, or the other way round.
fn layered_routers(reversed: bool) -> String {
    let mut text = String::from("from fastapi import APIRouter\n\n");
    for layer in 0..=7 {
        for name in ["a", "b"] {
            writeln!(text, "{name}{layer} = APIRouter()").unwrap();
        }
    }
    text.push_str("\n\n@a0.get(\"/leaf\")\ndef leaf():\n    return {}\n\n\n");
    let mut includes = Vec::new();
    for layer in 0..7 {
        for from in ["a", "b"] {
            for to in ["a", "b"] {
                includes.push(format!(
                    "{to}{}.include_router({from}{layer}, prefix=\"/{layer}{from}{to}\")\n",
                    layer + 1
                ));
            }
        }
    }
    if reversed {
        includes.reverse();
    }
    text.extend(includes);
    text
}

#[gsttt_scenario::scenario(
    "python",
    "A router reached in more than 64 ways is served in the first 64 by path"
)]
#[test]
fn a_router_reached_in_more_than_64_ways_is_served_in_the_first_64_by_path() {
    let (straight, reversed) = (layered_routers(false), layered_routers(true));
    let line = straight
        .lines()
        .position(|line| line.starts_with("@a0.get"))
        .unwrap()
        + 1;
    // A way is the choice of one of the two routers at each layer, the lowest layer in the lowest bit.
    let letter = |choice: u32, layer: usize| if (choice >> layer) & 1 == 0 { 'a' } else { 'b' };
    let mut every: Vec<String> = (0..1_u32 << 7)
        .map(|choice| {
            let mut path = String::new();
            for layer in (0..7).rev() {
                let from = if layer == 0 {
                    'a'
                } else {
                    letter(choice, layer - 1)
                };
                write!(path, "/{layer}{from}{}", letter(choice, layer)).unwrap();
            }
            format!("serves http GET {path}/leaf@{line}")
        })
        .collect();
    every.sort();
    every.truncate(64);
    for text in [&straight, &reversed] {
        let tree = index_files(&[
            ("pyproject.toml", FASTAPI_PROJECT),
            ("app.py", text.as_str()),
        ]);
        assert_eq!(tree.linked("app.py"), every);
    }
}

/// A call rule that serves HTTP is an entry of its module's URL list whichever pack it comes from, so a Django
/// include of the module puts its prefix in front, as it does for `path()`. Nothing else mounts the list, so a module
/// no urlconf includes keeps the path the call wrote.
#[test]
fn a_serving_call_of_a_pack_is_served_behind_the_includes_of_its_module() {
    let pack = r#"return {
        name = "aiohttp",
        language = "python",
        calls = { { callee = "web.get", kind = "http", role = "serves", key = "$arg1", method = "GET" } },
    }"#;
    let routes = "from aiohttp import web\n\nroutes = [web.get(\"/things\", handler)]\n";
    let packs = || {
        let mut packs = builtin_packs(PACKS).unwrap();
        packs.extend(builtin_packs(&[("aiohttp.lua", pack)]).unwrap());
        packs
    };
    // Nothing includes the module: the route keeps the path it was written with.
    let alone = index_files_with(
        &[
            ("pyproject.toml", "[project]\nname = \"app\"\n"),
            ("service/routes.py", routes),
        ],
        packs(),
    );
    assert_eq!(
        alone.linked("service/routes.py"),
        ["serves http GET /things@3"]
    );
    // A Django urlconf includes the module, as it does for a module of `path()` entries, so the prefix applies.
    let included = index_files_with(
        &[
            (
                "pyproject.toml",
                "[project]\nname = \"app\"\ndependencies = [\"django>=5\"]\n",
            ),
            (
                "urls.py",
                "from django.urls import include, path\n\nurlpatterns = [path(\"api/\", include(\"service.routes\"))]\n",
            ),
            ("service/__init__.py", ""),
            ("service/routes.py", routes),
        ],
        packs(),
    );
    assert_eq!(
        included.linked("service/routes.py"),
        ["serves http GET /api/things@3"]
    );
}
