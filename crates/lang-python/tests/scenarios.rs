//! The scenarios of the `python` capability. Each test indexes one fixture tree from `tests/fixtures/` through the
//! whole pipeline, checks what its scenario claims and snapshots the facts it is about, for review.
//!
//! `cfg(test)` on the whole file tells clippy that the helpers below are test code, where `clippy.toml` lets them panic.

#![cfg(test)]

use gsttt_config::pack::builtin_packs;
use gsttt_core::engine::pipeline::{self, FileRun, Module, Output};
use gsttt_core::fact::{
    ContractFact, Embedded, FileTags, ImportFlags, Line, ParseStatus, Resolution,
};
use gsttt_core::ids::FileId;
use gsttt_core::repo::RepoView;
use gsttt_lang_python::{PACKS, PYTHON};
use serde_json::{Value, json};

/// Larger than any fixture file.
const MAX_FILE_BYTES: u64 = 1 << 20;

/// A fixture tree after the pipeline ran on it, with the output as JSON for the snapshots.
struct Tree {
    repo: RepoView,
    output: Output,
    json: Value,
}

fn index(fixture: &str) -> Tree {
    let root = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    let repo = RepoView::scan(root).unwrap();
    let modules = [Module {
        connector: &PYTHON,
        options: Value::Null,
        packs: builtin_packs(PACKS).unwrap(),
    }];
    let output = pipeline::run(&repo, &modules, MAX_FILE_BYTES).unwrap();
    let json = output.to_json(&repo);
    Tree { repo, output, json }
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
    assert!(
        matches!(broken.facts.status, ParseStatus::Recovered { errors, .. } if errors > 0),
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
    ] {
        assert!(blind_spots.contains(&words), "{words}: {blind_spots:?}");
    }
    insta::assert_json_snapshot!("blind_spots_are_reported", tree.json["projects"]);
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
