//! The scenarios of the `go` capability. Each test indexes a fixture tree or a tree written in the test through the whole pipeline and checks what its scenario claims.
//!
//! `cfg(test)` on the whole file tells clippy that the helpers below are test code, where `clippy.toml` lets them panic.

#![cfg(test)]

use gsttt_config::pack::builtin_packs;
use gsttt_core::connector::{DynConnector, SourceFile};
use gsttt_core::engine::pipeline::{self, FileRun, Module, Output};
use gsttt_core::fact::{FileTags, Line, ParseStatus, Resolution, Use};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_lang_go::{GO, PACKS};
use serde_json::Value;

/// Larger than any fixture file.
const MAX_FILE_BYTES: u64 = 1 << 20;

/// A tree after the pipeline ran on it.
struct Tree {
    repo: RepoView,
    output: Output,
    /// The directory an inline tree was written to, kept until the test ends.
    _dir: Option<tempfile::TempDir>,
}

fn index(fixture: &str) -> Tree {
    let root = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    run(RepoView::scan(root).unwrap(), None)
}

/// A tree written to a temporary directory from `(path, text)` pairs and indexed like a fixture.
fn tree(files: &[(&str, &str)]) -> Tree {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let root = dir.path().to_str().unwrap().to_owned();
    run(RepoView::scan(root).unwrap(), Some(dir))
}

/// One module `example.com/m` with `files` below it.
fn module(files: &[(&str, &str)]) -> Tree {
    let mut all = vec![("go.mod", "module example.com/m\n\ngo 1.22\n")];
    all.extend_from_slice(files);
    tree(&all)
}

fn run(repo: RepoView, dir: Option<tempfile::TempDir>) -> Tree {
    let modules = [Module {
        connector: &GO,
        options: Value::Null,
        packs: builtin_packs(PACKS).unwrap(),
    }];
    let output = pipeline::run(&repo, &modules, MAX_FILE_BYTES).unwrap();
    Tree {
        repo,
        output,
        _dir: dir,
    }
}

impl Tree {
    fn path_of(&self, id: FileId) -> &str {
        self.repo.path(id).expect("the id names a file").as_str()
    }

    fn file(&self, path: &str) -> &FileRun {
        self.output
            .files
            .iter()
            .find(|file| self.path_of(file.id) == path)
            .expect("the tree holds the file")
    }

    fn claims(&self, path: &str) -> bool {
        self.output
            .files
            .iter()
            .any(|file| self.path_of(file.id) == path)
    }

    fn roots(&self) -> Vec<&str> {
        self.output
            .projects
            .iter()
            .map(|project| project.root.as_str())
            .collect()
    }

    /// The interfaces the link found for types, as `file: type -> file-of-interface Interface`.
    fn overrides(&self) -> Vec<String> {
        self.output
            .overrides
            .iter()
            .map(|found| {
                format!(
                    "{}: {} -> {} {}",
                    self.path_of(found.file),
                    found.name,
                    self.path_of(found.owner),
                    found.head
                )
            })
            .collect()
    }

    /// The declarations of a file as `name L<start>-<end>: uses`.
    fn declared(&self, path: &str) -> Vec<String> {
        let symbols = self.file(path).facts.symbols.as_ref().expect("symbols");
        symbols
            .declared
            .iter()
            .map(|d| format!("{} L{}-{}: {}", d.name, d.line, d.end, joined(&d.uses)))
            .collect()
    }

    /// The declaration named `name` as its uses, joined.
    fn uses(&self, path: &str, name: &str) -> String {
        let symbols = self.file(path).facts.symbols.as_ref().expect("symbols");
        symbols
            .declared
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| {
                panic!(
                    "{name} is not declared in {path}: {:?}",
                    self.declared(path)
                )
            })
            .uses
            .iter()
            .map(|used| used.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The member names a file reads on a value of unknown type.
    fn reads(&self, path: &str) -> Vec<String> {
        let symbols = self.file(path).facts.symbols.as_ref().expect("symbols");
        symbols.reads.iter().map(|read| read.name.clone()).collect()
    }

    /// The heirs of a file as `name of`.
    fn heirs(&self, path: &str) -> Vec<String> {
        let symbols = self.file(path).facts.symbols.as_ref().expect("symbols");
        symbols
            .heirs
            .iter()
            .map(|heir| format!("{} {}", heir.name, heir.of))
            .collect()
    }

    fn on_load(&self, path: &str) -> String {
        let symbols = self.file(path).facts.symbols.as_ref().expect("symbols");
        joined(&symbols.on_load)
    }

    /// The names a file exports, without the entry that records its package.
    fn exports(&self, path: &str) -> Vec<String> {
        self.file(path).facts.exports.clone().expect("exports")
    }

    /// The package clause of a file as the connector records it.
    fn package(&self, path: &str) -> String {
        self.file(path).facts.package.clone().expect("package")
    }

    /// Every import of a file as `spec [names] [exported] -> target`.
    fn imports(&self, path: &str) -> Vec<String> {
        self.file(path)
            .imports
            .iter()
            .map(|resolved| {
                let to = match &resolved.to {
                    Resolution::File { file } => self.path_of(*file).to_owned(),
                    Resolution::Files { files } => files
                        .iter()
                        .map(|file| self.path_of(*file))
                        .collect::<Vec<_>>()
                        .join(" + "),
                    Resolution::External { package } => format!("package {package}"),
                    Resolution::Builtin => "builtin".to_owned(),
                    Resolution::Unresolved(reason) => format!("unresolved {reason:?}"),
                };
                format!(
                    "{} [{}] [{}] -> {to}",
                    resolved.import.spec,
                    resolved.import.names.join(","),
                    resolved.import.exported.join(",")
                )
            })
            .collect()
    }

    /// The contract facts of a file as `<role> <key>@<line>`.
    fn contracts(&self, path: &str) -> Vec<String> {
        self.file(path)
            .facts
            .contracts
            .iter()
            .map(|fact| format!("{} {}@{}", fact.role, fact.key, fact.line))
            .collect()
    }

    fn embedded(&self, path: &str) -> Vec<String> {
        self.file(path)
            .facts
            .embedded
            .iter()
            .map(|doc| format!("{} L{} {}", doc.lang, doc.line, doc.text))
            .collect()
    }

    fn unresolved_lines(&self, path: &str) -> Vec<Line> {
        self.file(path)
            .facts
            .unresolved
            .iter()
            .map(|s| s.line)
            .collect()
    }

    fn tags(&self, path: &str) -> FileTags {
        self.file(path).facts.tags
    }
}

/// The status of a file's parse.
/// The names of `uses`, joined by a space.
fn joined(uses: &[Use]) -> String {
    uses.iter()
        .map(|used| used.name.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn status(tree: &Tree, path: &str) -> ParseStatus {
    tree.file(path).facts.status.clone()
}

// Projects.

#[gsttt_scenario::scenario("go", "A module is a project named by its go.mod")]
#[test]
fn a_module_is_a_project_named_by_its_go_mod() {
    let t = module(&[
        (
            "main.go",
            "package main\n\nimport \"example.com/m/util\"\n\nfunc main() { util.Do() }\n",
        ),
        ("util/util.go", "package util\n\nfunc Do() {}\n"),
    ]);
    assert_eq!(t.roots(), [""]);
    assert!(t.claims("main.go") && t.claims("util/util.go") && t.claims("go.mod"));
    // The module line is what the project resolves the imports of its own packages against.
    assert_eq!(
        t.imports("main.go"),
        ["example.com/m/util [Do] [util.Do] -> util/util.go"]
    );
}

#[gsttt_scenario::scenario("go", "A nested module owns what is below it")]
#[test]
fn a_nested_module_owns_what_is_below_it() {
    let t = tree(&[
        ("go.mod", "module example.com/m\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/tools/gen\"\n\nfunc main() { gen.Run() }\n",
        ),
        ("tools/go.mod", "module example.com/tools\n"),
        ("tools/gen/gen.go", "package gen\n\nfunc Run() {}\n"),
        ("tools/gen/other.go", "package gen\n\nfunc Other() {}\n"),
    ]);
    assert_eq!(t.roots(), ["", "tools"]);
    assert!(t.claims("tools/gen/gen.go") && t.claims("main.go"));
    assert_eq!(
        t.imports("main.go"),
        ["example.com/tools/gen [Run] [gen.Run] -> tools/gen/gen.go"]
    );
}

#[gsttt_scenario::scenario("go", "Vendored code and test data are not read")]
#[test]
fn vendored_code_and_test_data_are_not_read() {
    let t = module(&[
        ("main.go", "package main\n"),
        ("vendor/x/x.go", "package x\n"),
        ("testdata/t.go", "package t\n"),
        ("pkg/testdata/t.go", "package t\n"),
        ("_scratch/s.go", "package s\n"),
        (".hidden/h.go", "package h\n"),
        ("pkg/_skip.go", "package pkg\n"),
        ("pkg/.skip.go", "package pkg\n"),
        ("pkg/ok.go", "package pkg\n"),
    ]);
    for path in [
        "vendor/x/x.go",
        "testdata/t.go",
        "pkg/testdata/t.go",
        "_scratch/s.go",
        ".hidden/h.go",
        "pkg/_skip.go",
        "pkg/.skip.go",
    ] {
        assert!(!t.claims(path), "{path}");
    }
    assert!(t.claims("main.go") && t.claims("pkg/ok.go"));
}

#[gsttt_scenario::scenario("go", "A test file is tagged")]
#[test]
fn a_test_file_is_tagged() {
    let t = module(&[("a.go", "package a\n"), ("a_test.go", "package a\n")]);
    assert!(t.tags("a_test.go").contains(FileTags::TEST));
    assert!(!t.tags("a.go").contains(FileTags::TEST));
    assert!(t.tags("go.mod").contains(FileTags::MANIFEST));
}

// Imports.

#[gsttt_scenario::scenario("go", "An import of the standard library is built in")]
#[test]
fn an_import_of_the_standard_library_is_built_in() {
    let t = module(&[(
        "main.go",
        "package main\n\nimport (\n\t\"fmt\"\n\t\"net/http\"\n\t\"C\"\n)\n\nfunc main() { fmt.Println(http.StatusOK) }\n",
    )]);
    assert_eq!(
        t.imports("main.go"),
        [
            "fmt [Println] [fmt.Println] -> builtin",
            "net/http [StatusOK] [http.StatusOK] -> builtin",
            "C [] [] -> builtin",
        ]
    );
}

#[gsttt_scenario::scenario("go", "An import of a required module is external")]
#[test]
fn an_import_of_a_required_module_is_external() {
    let t = tree(&[
        (
            "go.mod",
            "module example.com/m\n\nrequire (\n\tgithub.com/go-chi/chi/v5 v5.0.12\n\tgopkg.in/yaml.v3 v3.0.1 // indirect\n)\n",
        ),
        (
            "main.go",
            "package main\n\nimport (\n\t\"github.com/go-chi/chi/v5\"\n\t\"github.com/go-chi/chi/v5/middleware\"\n\t\"gopkg.in/yaml.v3\"\n\t\"github.com/other/lib\"\n)\n\nfunc main() { chi.NewRouter(); middleware.Logger(); yaml.Marshal(); lib.Do() }\n",
        ),
    ]);
    assert_eq!(
        t.imports("main.go"),
        [
            "github.com/go-chi/chi/v5 [NewRouter] [chi.NewRouter] -> package github.com/go-chi/chi/v5",
            "github.com/go-chi/chi/v5/middleware [Logger] [middleware.Logger] -> package github.com/go-chi/chi/v5",
            "gopkg.in/yaml.v3 [Marshal] [yaml.Marshal] -> package gopkg.in/yaml.v3",
            "github.com/other/lib [Do] [lib.Do] -> package github.com/other/lib",
        ]
    );
}

const PKG_A: [(&str, &str); 4] = [
    ("a/x.go", "package a\n\nfunc Foo() {}\n"),
    ("a/y.go", "package a\n\nfunc Bar() {}\n"),
    ("a/z_test.go", "package a\n\nfunc Baz() {}\n"),
    ("a/doc.go", "package a\n"),
];

#[gsttt_scenario::scenario(
    "go",
    "An import of a package of the module reaches only the files that declare what the file reads"
)]
#[test]
fn an_import_of_a_package_of_the_module_reaches_only_the_files_that_declare_what_the_file_reads() {
    let mut files = PKG_A.to_vec();
    files.push((
        "main.go",
        "package main\n\nimport \"example.com/m/a\"\n\nfunc main() { a.Foo() }\n",
    ));
    let t = module(&files);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/m/a [Foo] [a.Foo] -> a/x.go"]
    );
}

#[gsttt_scenario::scenario("go", "An import reaches the files that run when the package loads")]
#[test]
fn an_import_reaches_the_files_that_run_when_the_package_loads() {
    let mut files = PKG_A.to_vec();
    files.push(("a/boot.go", "package a\n\nfunc init() { Bar() }\n"));
    files.push((
        "main.go",
        "package main\n\nimport \"example.com/m/a\"\n\nfunc main() { a.Foo() }\n",
    ));
    let t = module(&files);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/m/a [Foo] [a.Foo] -> a/boot.go + a/x.go"]
    );
    assert_eq!(t.on_load("a/boot.go"), "Bar");
    assert_eq!(t.declared("a/boot.go"), Vec::<String>::new());
}

#[gsttt_scenario::scenario("go", "An import with an alias binds the alias")]
#[test]
fn an_import_with_an_alias_binds_the_alias() {
    let mut files = PKG_A.to_vec();
    files.push((
        "main.go",
        "package main\n\nimport pkg \"example.com/m/a\"\n\nfunc main() { pkg.Foo(); a := 1; _ = a }\n",
    ));
    let t = module(&files);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/m/a [Foo] [pkg.Foo] -> a/x.go"]
    );
    assert_eq!(t.uses("main.go", "main"), "pkg.Foo");
}

#[gsttt_scenario::scenario(
    "go",
    "A package whose name is not the last segment of its path is followed whole"
)]
#[test]
fn a_package_whose_name_is_not_the_last_segment_of_its_path_is_followed_whole() {
    let t = module(&[
        ("lib/x.go", "package other\n\nfunc Foo() {}\n"),
        ("lib/y.go", "package other\n\nfunc Bar() {}\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/m/lib\"\n\nfunc main() { other.Foo() }\n",
        ),
        (
            "versioned.go",
            "package main\n\nimport \"example.com/m/lib\"\n\nfunc v() { lib.Foo() }\n",
        ),
    ]);
    // `other` is read through no import, so it stays a name of the package: no sibling declares it, and the edge is empty.
    assert_eq!(
        t.imports("main.go"),
        [
            "example.com/m/lib [] [] -> lib/x.go + lib/y.go",
            ". [other] [other] -> "
        ]
    );
    assert_eq!(
        t.imports("versioned.go"),
        ["example.com/m/lib [Foo] [lib.Foo] -> lib/x.go"]
    );
}

#[gsttt_scenario::scenario("go", "A blank import reaches every file of the package")]
#[test]
fn a_blank_import_reaches_every_file_of_the_package() {
    let mut files = PKG_A.to_vec();
    files.push(("main.go", "package main\n\nimport _ \"example.com/m/a\"\n"));
    let t = module(&files);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/m/a [] [] -> a/doc.go + a/x.go + a/y.go"]
    );
    assert_eq!(
        t.file("main.go").imports.first().unwrap().import.kind,
        gsttt_core::fact::ImportKind::SideEffect
    );
}

#[gsttt_scenario::scenario("go", "A package of a nested module is reached")]
#[test]
fn a_package_of_a_nested_module_is_reached() {
    let t = tree(&[
        ("go.mod", "module example.com/m\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/tools/gen\"\n\nfunc main() { gen.Run() }\n",
        ),
        ("tools/go.mod", "module example.com/tools\n"),
        ("tools/gen/gen.go", "package gen\n\nfunc Run() {}\n"),
        ("tools/gen/gen_test.go", "package gen\n"),
    ]);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/tools/gen [Run] [gen.Run] -> tools/gen/gen.go"]
    );
}

// Files of one package.

#[gsttt_scenario::scenario("go", "A file reaches the sibling that declares a name it reads")]
#[test]
fn a_file_reaches_the_sibling_that_declares_a_name_it_reads() {
    let t = module(&[
        ("a.go", "package m\n\nfunc A() { helper() }\n"),
        ("b.go", "package m\n\nfunc helper() {}\n"),
    ]);
    assert_eq!(t.imports("a.go"), [". [helper] [helper] -> b.go"]);
}

#[gsttt_scenario::scenario("go", "A file records the members it reads on a value of unknown type")]
#[test]
fn a_file_records_the_members_it_reads_on_a_value_of_unknown_type() {
    let t = module(&[(
        "a.go",
        "package m

import \"fmt\"

type T struct{ Field int }

func (t *T) Known() {}

var pkgVar = other()

func other() *T { return nil }

func f() *T { return nil }

func Go(t *T, items []*T) {
	x := f()
	x.A()
	f().B()
	items[0].C()
	pkgVar.D()
	fmt.Println()
	t.Known()
	_ = T{Field: 1}
}
",
    )]);
    assert_eq!(t.reads("a.go"), ["A", "B", "C", "D"]);
}

#[gsttt_scenario::scenario("go", "A file does not reach a sibling it reads nothing from")]
#[test]
fn a_file_does_not_reach_a_sibling_it_reads_nothing_from() {
    let t = module(&[
        ("a.go", "package m\n\nfunc A() {}\n"),
        ("b.go", "package m\n\nfunc B() {}\n"),
    ]);
    assert_eq!(t.imports("a.go"), Vec::<String>::new());
    assert_eq!(t.imports("b.go"), Vec::<String>::new());
}

#[gsttt_scenario::scenario("go", "A file that is no test does not reach a test file")]
#[test]
fn a_file_that_is_no_test_does_not_reach_a_test_file() {
    let t = module(&[
        ("a.go", "package m\n\nfunc A() { fixture() }\n"),
        (
            "a_test.go",
            "package m\n\nfunc fixture() {}\n\nfunc TestA() { A() }\n",
        ),
    ]);
    assert_eq!(t.imports("a.go"), [". [fixture] [fixture] -> "]);
    assert_eq!(t.imports("a_test.go"), [". [A] [A] -> a.go"]);
}

#[gsttt_scenario::scenario("go", "A local name that shadows a sibling's name is no use of it")]
#[test]
fn a_local_name_that_shadows_a_siblings_name_is_no_use_of_it() {
    let t = module(&[
        (
            "a.go",
            "package m\n\nfunc A(limit int) int {\n\tcount := 1\n\tvar total int\n\tfor index := range 3 {\n\t\ttotal += index\n\t}\n\treturn count + limit + total\n}\n",
        ),
        ("b.go", "package m\n\nvar count, limit, total, index int\n"),
    ]);
    assert!(t.imports("a.go").is_empty(), "{:?}", t.imports("a.go"));
    assert_eq!(t.uses("a.go", "A"), "");
}

#[gsttt_scenario::scenario("go", "A package that declares its own min is read")]
#[test]
fn a_package_that_declares_its_own_min_is_read() {
    let t = module(&[
        ("a.go", "package m\n\nfunc A() int { return min(1, 2) }\n"),
        ("b.go", "package m\n\nfunc min(a, b int) int { return a }\n"),
        ("c.go", "package m\n\nfunc C() int { return max(1, 2) }\n"),
    ]);
    assert_eq!(t.imports("a.go"), [". [min] [min] -> b.go"]);
    assert_eq!(t.imports("c.go"), [". [max] [max] -> "]);
}

#[gsttt_scenario::scenario("go", "A struct field key is no use")]
#[test]
fn a_struct_field_key_is_no_use() {
    let t = module(&[
        (
            "a.go",
            "package m\n\ntype T struct{ name int }\n\nfunc A() T { return T{name: 1} }\n\nfunc B() map[string]int { return map[string]int{name: 1} }\n",
        ),
        ("b.go", "package m\n\nconst name = \"n\"\n"),
    ]);
    assert_eq!(t.uses("a.go", "A"), "T");
    assert_eq!(t.uses("a.go", "B"), "name");
}

// Declarations.

const KINDS: &str = "package m

import \"fmt\"

type Box[K comparable] struct {
	Key  K
	name string
	Inner
}

type Inner struct{}

type Greeter interface {
	Greet(who string) string
	fmt.Stringer
}

type ID = int

const Limit = 10

const (
	A, B = 1, 2
	C    = fmt.Sprint(Limit)
)

var Default = Box[string]{}

func Make() {}

func (b *Box[K]) Get() K { return b.Key }

func (Inner) Hello() {}
";

#[gsttt_scenario::scenario(
    "go",
    "Functions, methods, types, fields, constants and variables are declared"
)]
#[test]
fn functions_methods_types_fields_constants_and_variables_are_declared() {
    let t = module(&[("kinds.go", KINDS)]);
    let names: Vec<_> = t
        .declared("kinds.go")
        .iter()
        .map(|line| line.split(':').next().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Box L5-9",
            "Box.Key L6-6",
            "Box.name L7-7",
            "Box.Inner L8-8",
            "Inner L11-11",
            "Greeter L13-16",
            "Greeter.Greet L14-14",
            "ID L18-18",
            "Limit L20-20",
            "A L23-23",
            "B L23-23",
            "C L24-24",
            "Default L27-27",
            "Make L29-29",
            "Box.Get L31-31",
            "Inner.Hello L33-33",
        ]
    );
    // A struct carries no use of its field types; they belong to the fields.
    assert_eq!(t.uses("kinds.go", "Box"), "");
    assert_eq!(t.uses("kinds.go", "Box.Inner"), "Inner");
    assert_eq!(t.uses("kinds.go", "C"), "Limit fmt.Sprint");
    assert_eq!(t.uses("kinds.go", "Default"), "Box");
}

#[gsttt_scenario::scenario("go", "A generic receiver names the type")]
#[test]
fn a_generic_receiver_names_the_type() {
    let t = module(&[(
        "box.go",
        "package m\n\ntype Box[K any] struct{ key K }\n\nfunc (b *Box[K]) Get() K { return b.key }\n\nfunc (Box[_]) Zero() {}\n",
    )]);
    let names: Vec<_> = t
        .declared("box.go")
        .iter()
        .map(|l| l.split(' ').next().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Box", "Box.key", "Box.Get", "Box.Zero"]);
    assert_eq!(t.uses("box.go", "Box.Get"), "Box Box.key");
}

#[gsttt_scenario::scenario("go", "init declares nothing and its uses run on load")]
#[test]
fn init_declares_nothing_and_its_uses_run_on_load() {
    let t = module(&[(
        "boot.go",
        "package m\n\nfunc main() { run() }\n\nfunc init() {\n\tregister()\n\tsetup := 1\n\t_ = setup\n}\n",
    )]);
    let names: Vec<_> = t
        .declared("boot.go")
        .iter()
        .map(|l| l.split(' ').next().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["main"]);
    assert_eq!(t.on_load("boot.go"), "register");
    assert_eq!(t.uses("boot.go", "main"), "run");
}

#[gsttt_scenario::scenario("go", "A package-level initialiser runs on load")]
#[test]
fn a_package_level_initialiser_runs_on_load() {
    let t = module(&[(
        "vars.go",
        "package m\n\nvar table = build()\n\nvar _ = hook()\n\nvar lazy = func() int { return later() }\n",
    )]);
    assert_eq!(t.uses("vars.go", "table"), "build");
    assert_eq!(t.on_load("vars.go"), "build hook");
    assert_eq!(t.uses("vars.go", "lazy"), "later");
}

#[gsttt_scenario::scenario("go", "Every top-level name is exported, lowercase too")]
#[test]
fn every_top_level_name_is_exported_lowercase_too() {
    let t = module(&[("kinds.go", KINDS)]);
    assert_eq!(
        t.exports("kinds.go"),
        [
            "A", "B", "Box", "C", "Default", "Greeter", "ID", "Inner", "Limit", "Make"
        ]
    );
    let lower = module(&[("l.go", "package m\n\nfunc helper() {}\n\nvar count int\n")]);
    assert_eq!(lower.exports("l.go"), ["count", "helper"]);
}

// What a declaration uses.

#[gsttt_scenario::scenario("go", "A function uses what it calls by package and by name")]
#[test]
fn a_function_uses_what_it_calls_by_package_and_by_name() {
    let t = module(&[(
        "f.go",
        "package m\n\nimport (\n\t\"fmt\"\n\tstr \"strings\"\n)\n\nfunc F() {\n\thelper()\n\tfmt.Println(str.ToUpper(name))\n\tfmt.Println()\n}\n",
    )]);
    assert_eq!(t.uses("f.go", "F"), "fmt.Println helper name str.ToUpper");
}

#[gsttt_scenario::scenario("go", "A parameter's type is a use of the type and of no member")]
#[test]
fn a_parameters_type_is_a_use_of_the_type_and_of_no_member() {
    let t = module(&[
        (
            "p.go",
            "package m\n\nimport \"example.com/m/inv\"\n\nfunc F(h Header, i inv.Invoice) (out Footer) { return }\n",
        ),
        ("inv/i.go", "package inv\n\ntype Invoice struct{}\n"),
    ]);
    assert_eq!(t.uses("p.go", "F"), "Footer Header inv.Invoice");
}

#[gsttt_scenario::scenario("go", "A method read through a receiver is that method")]
#[test]
fn a_method_read_through_a_receiver_is_that_method() {
    let t = module(&[(
        "r.go",
        "package m\n\ntype S struct{}\n\nfunc (s *S) A() { s.B() }\n\nfunc (s *S) B() {}\n",
    )]);
    assert_eq!(t.uses("r.go", "S.A"), "S S.B");
}

#[gsttt_scenario::scenario(
    "go",
    "A method read through a parameter, a local, a constructor and a range variable is that method"
)]
#[test]
fn a_method_read_through_a_parameter_a_local_a_constructor_and_a_range_variable_is_that_method() {
    let t = module(&[(
        "h.go",
        "package m\n\nfunc ByParam(h Header) { h.Title() }\n\nfunc ByLiteral() {\n\th := Header{}\n\th.Title()\n}\n\nfunc ByPointer() {\n\th := &Header{}\n\th.Title()\n}\n\nfunc ByNew() {\n\th := new(Header)\n\th.Title()\n}\n\nfunc ByRange(all []Header) {\n\tfor _, h := range all {\n\t\th.Title()\n\t}\n}\n\nfunc ByIndex(all []Header) {\n\th := all[0]\n\th.Title()\n}\n\nfunc ByCopy(h Header) {\n\tg := h\n\tg.Title()\n}\n\nfunc ByAssertion(x any) {\n\th := x.(Header)\n\th.Title()\n}\n\nfunc ByVar() {\n\tvar h Header\n\th.Title()\n}\n",
    )]);
    for (name, expected) in [
        ("ByParam", "Header Header.Title"),
        ("ByLiteral", "Header Header.Title"),
        ("ByPointer", "Header Header.Title"),
        ("ByNew", "Header Header.Title new"),
        ("ByRange", "Header Header.Title"),
        ("ByIndex", "Header Header.Title"),
        ("ByCopy", "Header Header.Title"),
        ("ByAssertion", "Header Header.Title"),
        ("ByVar", "Header Header.Title"),
    ] {
        assert_eq!(t.uses("h.go", name), expected, "{name}");
    }
}

#[gsttt_scenario::scenario(
    "go",
    "A method read through a field of a struct of the file is that method"
)]
#[test]
fn a_method_read_through_a_field_of_a_struct_of_the_file_is_that_method() {
    let t = module(&[(
        "s.go",
        "package m\n\ntype Service struct {\n\tdb    *DB\n\tstore Store\n}\n\nfunc (s *Service) Run() {\n\ts.db.Query()\n\ts.store.Save()\n}\n",
    )]);
    assert_eq!(
        t.uses("s.go", "Service.Run"),
        "DB.Query Service Service.db Service.store Store.Save"
    );
}

#[gsttt_scenario::scenario("go", "A value passed on is a use of the type")]
#[test]
fn a_value_passed_on_is_a_use_of_the_type() {
    let t = module(&[(
        "v.go",
        "package m\n\nfunc F() {\n\th := Header{}\n\tconsume(h)\n\tg := &Footer{}\n\tconsume(g)\n}\n",
    )]);
    assert_eq!(t.uses("v.go", "F"), "Footer Header consume");
}

#[gsttt_scenario::scenario("go", "A method read through a value of no known type is not recorded")]
#[test]
fn a_method_read_through_a_value_of_no_known_type_is_not_recorded() {
    let t = module(&[(
        "u.go",
        "package m\n\nfunc F() {\n\tx := mystery()\n\tx.Run()\n\ty := x.Next()\n\ty.Stop()\n}\n",
    )]);
    assert_eq!(t.uses("u.go", "F"), "mystery");
}

/// Indexes one file of module `example.com/m` and returns the member names it reads.
fn reads_of(source: &str) -> Vec<String> {
    module(&[("a.go", source)]).reads("a.go")
}

#[gsttt_scenario::scenario("go", "A chain on a value of a standard library type reads no member")]
#[test]
fn a_chain_on_a_value_of_a_standard_library_type_reads_no_member() {
    let source = "package m

import \"net/http\"

type Context struct {
	Request *http.Request
}

func Direct(w http.ResponseWriter) { w.Header().Get(\"x\") }

func Field(r *http.Request) { r.Header.Get(\"x\") }

func Through(c *Context) { c.Request.Header.Set(\"a\", \"b\") }
";
    assert_eq!(reads_of(source), Vec::<String>::new());
}

#[test]
fn a_call_chain_on_a_response_writer_reads_no_member() {
    let source = "package m

import \"net/http\"

func F(w http.ResponseWriter) { w.Header().Get(\"x\") }
";
    assert_eq!(reads_of(source), Vec::<String>::new());
}

#[test]
fn a_field_chain_on_a_request_reads_no_member() {
    let source = "package m

import \"net/http\"

func F(r *http.Request) { r.Header.Get(\"x\") }
";
    assert_eq!(reads_of(source), Vec::<String>::new());
}

#[test]
fn a_chain_through_a_local_struct_field_of_a_standard_library_type_reads_no_member() {
    let source = "package m

import \"net/http\"

type Context struct {
	Request *http.Request
}

func F(c *Context) { c.Request.Header.Set(\"a\", \"b\") }
";
    assert_eq!(reads_of(source), Vec::<String>::new());
}

#[gsttt_scenario::scenario(
    "go",
    "A name bound to a call of the standard library stands outside the module"
)]
#[test]
fn a_name_bound_to_a_call_of_the_standard_library_stands_outside_the_module() {
    let source = "package m

import (
	\"net/http\"
	\"net/http/httptest\"
	\"os\"
)

func Request() {
	req, _ := http.NewRequest(\"GET\", \"/\", nil)
	req.Header.Set(\"a\", \"b\")
}

func Recorder() {
	w := httptest.NewRecorder()
	w.Header().Get(\"x\")
}

func File() {
	file, err := os.CreateTemp(\"\", \"\")
	file.Name()
	_ = err
}
";
    assert_eq!(reads_of(source), Vec::<String>::new());
}

#[test]
fn a_name_assigned_to_something_else_later_is_read_again() {
    let source = "package m

import \"net/http\"

func F() {
	svc := other()
	req, _ := http.NewRequest(\"GET\", \"/\", nil)
	req = svc.Make()
	req.Header.Set(\"a\", \"b\")
}
";
    assert_eq!(reads_of(source), ["Header", "Make", "Set"]);
}

#[test]
fn a_local_named_like_the_package_is_read() {
    let source = "package m

import \"net/http\"

func F() {
	http := local()
	http.Get()
}
";
    assert_eq!(reads_of(source), ["Get"]);
}

#[gsttt_scenario::scenario("go", "A chain on a value that may be of a type here is read")]
#[test]
fn a_chain_on_a_value_that_may_be_of_a_type_here_is_read() {
    let source = "package m

import \"sync/atomic\"

func F[T any](p atomic.Pointer[T]) { p.Load().Place() }
";
    assert_eq!(reads_of(source), ["Place"]);
}

#[test]
fn a_variable_of_a_package_outside_the_module_stands_outside() {
    let source = "package m

import (
	\"net/http\"
	\"os\"

	\"github.com/acme/pkg\"
)

func Client() {
	http.DefaultClient.Get(\"/\")
}

func Out() {
	os.Stdout.Write(nil)
}

func Third() {
	pkg.Default.Place()
}

func Shadowed(os Box) {
	os.Stdout.Close()
}
";
    assert_eq!(reads_of(source), ["Close"]);
}

#[test]
fn a_call_of_a_generic_standard_library_package_gives_no_outside_value() {
    let source = "package m

import \"slices\"

func Direct(xs []T) { slices.Max(xs).Place() }

func Named(xs []T) {
	x := slices.Max(xs)
	x.Place()
}
";
    assert_eq!(reads_of(source), ["Place"]);
}

#[gsttt_scenario::scenario("go", "A value that no call of the standard library gives is read")]
#[test]
fn a_call_of_a_third_party_package_gives_no_outside_value() {
    let source = "package m

import \"github.com/samber/lo\"

func F(y Y) {
	x := lo.Must(y)
	x.Place()
}
";
    assert_eq!(reads_of(source), ["Place"]);
}

#[gsttt_scenario::scenario("go", "A type-asserted value stands on the asserted type")]
#[test]
fn a_type_asserted_value_is_a_use_of_the_asserted_type() {
    let t = module(&[(
        "a.go",
        "package m

import \"context\"

type T struct{}

func (t *T) Place() {}

func F(ctx context.Context, k string) {
	v := ctx.Value(k).(*T)
	v.Place()
}
",
    )]);
    assert_eq!(t.reads("a.go"), Vec::<String>::new());
    assert_eq!(
        t.uses("a.go", "F"),
        "T T.Place context.Context context.Context.Value"
    );
}

#[test]
fn a_package_of_another_module_of_the_repository_is_not_outside() {
    let t = tree(&[
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        ("other/go.mod", "module example.com/other\n\ngo 1.22\n"),
        ("other/lib/lib.go", "package lib\n\ntype T struct{}\n"),
        (
            "a.go",
            "package m

import \"example.com/other/lib\"

func F(x lib.T) { x.Read().Place() }
",
        ),
    ]);
    assert_eq!(t.reads("a.go"), ["Place"]);
}

#[gsttt_scenario::scenario("go", "The arguments of a chain on an outside value are read")]
#[test]
fn the_arguments_of_a_chain_on_an_outside_value_are_read() {
    let source = "package m

import \"net/http\"

func F(w http.ResponseWriter) {
	svc := other()
	w.Header().Set(\"a\", svc.Name())
}
";
    assert_eq!(reads_of(source), ["Name"]);
}

#[gsttt_scenario::scenario("go", "A shadowing local is no use of the package")]
#[test]
fn a_shadowing_local_is_no_use_of_the_package() {
    let t = module(&[(
        "s.go",
        "package m\n\nimport \"fmt\"\n\nfunc F() {\n\tfmt := Printer{}\n\tfmt.Println()\n}\n\nfunc G() {\n\tfmt.Println()\n}\n",
    )]);
    assert_eq!(t.uses("s.go", "F"), "Printer Printer.Println");
    assert_eq!(t.uses("s.go", "G"), "fmt.Println");
}

// Environment variables, HTTP clients, HTTP servers, raw SQL.

#[gsttt_scenario::scenario("go", "os.Getenv with a literal")]
#[test]
fn os_getenv_with_a_literal() {
    let t = module(&[(
        "env.go",
        "package m\n\nimport \"os\"\n\nfunc F(name string) {\n\t_ = os.Getenv(\"API_URL\")\n\t_, _ = os.LookupEnv(\"TOKEN\")\n\t_ = os.Setenv(\"MODE\", \"x\")\n\t_ = os.Getenv(name)\n}\n",
    )]);
    assert_eq!(
        t.contracts("env.go"),
        [
            "calls env API_URL@6",
            "calls env TOKEN@7",
            "serves env MODE@8"
        ]
    );
    assert_eq!(t.unresolved_lines("env.go"), [9]);
}

#[gsttt_scenario::scenario("go", "A URL with a computed base is read as a path")]
#[test]
fn a_url_with_a_computed_base_is_read_as_a_path() {
    let t = module(&[(
        "c.go",
        "package main\n\nimport \"net/http\"\n\nfunc F(base string) {\n\thttp.Get(base + \"/api/users\")\n\thttp.Post(base+\"/api/orders\", \"application/json\", nil)\n\thttp.Get(base)\n}\n",
    )]);
    assert_eq!(
        t.contracts("c.go"),
        [
            "calls http GET /api/users@6",
            "calls http POST /api/orders@7"
        ]
    );
    assert_eq!(t.unresolved_lines("c.go"), [8]);
}

#[gsttt_scenario::scenario("go", "An alias of net/http is read")]
#[test]
fn an_alias_of_net_http_is_read() {
    let t = module(&[(
        "a.go",
        "package main\n\nimport nethttp \"net/http\"\n\nfunc F() {\n\tnethttp.Get(\"https://example.com/x\")\n\tnethttp.NewRequest(\"DELETE\", \"/items/1\", nil)\n}\n",
    )]);
    assert_eq!(
        t.contracts("a.go"),
        ["calls http GET /x@6", "calls http DELETE /items/1@7"]
    );
}

#[gsttt_scenario::scenario("go", "A local package named http is not read as net/http")]
#[test]
fn a_local_package_named_http_is_not_read_as_net_http() {
    let t = module(&[
        ("http/h.go", "package http\n\nfunc Get(url string) {}\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/m/http\"\n\nfunc F() { http.Get(\"/api/users\") }\n",
        ),
    ]);
    assert_eq!(t.contracts("main.go"), Vec::<String>::new());
    assert_eq!(t.unresolved_lines("main.go"), Vec::<u32>::new());
}

#[gsttt_scenario::scenario("go", "A constant of the file fills the URL")]
#[test]
fn a_constant_of_the_file_fills_the_url() {
    let t = module(&[(
        "k.go",
        "package main\n\nimport \"net/http\"\n\nconst users = \"/api/users\"\n\nvar orders = \"/api/orders\"\n\nfunc F() {\n\tbase := \"/api\"\n\thttp.Get(users)\n\thttp.Get(orders)\n\thttp.Get(base + \"/items\")\n}\n",
    )]);
    assert_eq!(
        t.contracts("k.go"),
        [
            "calls http GET /api/users@11",
            "calls http GET /api/orders@12",
            "calls http GET /api/items@13"
        ]
    );
}

#[gsttt_scenario::scenario("go", "fmt.Sprintf fills the URL")]
#[test]
fn fmt_sprintf_fills_the_url() {
    let t = module(&[(
        "f.go",
        "package main\n\nimport (\n\t\"fmt\"\n\t\"net/http\"\n)\n\nfunc F(id int) {\n\thttp.Get(fmt.Sprintf(\"/api/users/%d\", id))\n\thttp.Get(fmt.Sprintf(\"%s/api/users/%s/orders\", \"https://x.test\", \"a\"))\n}\n",
    )]);
    assert_eq!(
        t.contracts("f.go"),
        [
            "calls http GET /api/users/{}@9",
            "calls http GET /api/users/a/orders@10"
        ]
    );
}

#[gsttt_scenario::scenario("go", "A method pattern of Go 1.22 gives the method and the path")]
#[test]
fn a_method_pattern_of_go_1_22_gives_the_method_and_the_path() {
    let t = module(&[(
        "s.go",
        "package main\n\nimport \"net/http\"\n\nfunc F(mux *http.ServeMux) {\n\tmux.HandleFunc(\"GET /users/{id}\", h)\n\tmux.HandleFunc(\"/health\", h)\n\thttp.HandleFunc(\"POST /orders\", h)\n\thttp.Handle(\"/static/\", nil)\n}\n",
    )]);
    assert_eq!(
        t.contracts("s.go"),
        [
            "serves http GET /users/{}@6",
            "serves http * /health@7",
            "serves http POST /orders@8",
            "serves http * /static@9"
        ]
    );
}

#[gsttt_scenario::scenario("go", "A query is read as SQL")]
#[test]
fn a_query_is_read_as_sql() {
    let t = module(&[(
        "q.go",
        "package main\n\nfunc (s *Store) F() {\n\ts.db.Query(\"SELECT id FROM orders\")\n\ts.db.Exec(\"DELETE FROM items WHERE id = 1\")\n}\n",
    )]);
    assert_eq!(
        t.embedded("q.go"),
        [
            "sql L4 SELECT id FROM orders",
            "sql L5 DELETE FROM items WHERE id = 1"
        ]
    );
}

#[gsttt_scenario::scenario("go", "QueryContext reads its second argument")]
#[test]
fn query_context_reads_its_second_argument() {
    let t = module(&[(
        "q.go",
        "package main\n\nfunc F(ctx Context, db *DB) {\n\tdb.QueryContext(ctx, \"SELECT id FROM orders\")\n\tdb.ExecContext(ctx, \"UPDATE orders SET x = 1\")\n}\n",
    )]);
    assert_eq!(
        t.embedded("q.go"),
        [
            "sql L4 SELECT id FROM orders",
            "sql L5 UPDATE orders SET x = 1"
        ]
    );
}

#[gsttt_scenario::scenario("go", "A call with no text in its first argument records nothing")]
#[test]
fn a_call_with_no_text_in_its_first_argument_records_nothing() {
    let t = module(&[(
        "n.go",
        "package main\n\nfunc F(q string, db *DB) {\n\tdb.Query(q)\n\tdb.Exec(build())\n\tcache.Get(1)\n}\n",
    )]);
    assert_eq!(t.embedded("n.go"), Vec::<String>::new());
    assert_eq!(t.unresolved_lines("n.go"), Vec::<u32>::new());
}

// Syntax errors.

#[gsttt_scenario::scenario("go", "A file with a syntax error is recovered")]
#[test]
fn a_file_with_a_syntax_error_is_recovered() {
    let t = module(&[
        ("good.go", "package m\n\nfunc Good() {}\n"),
        ("bad.go", "package m\n\nfunc Bad( {\n\tgood()\n}\n"),
    ]);
    match status(&t, "bad.go") {
        ParseStatus::Recovered {
            errors,
            first_error_line,
        } => {
            assert!(errors >= 1);
            assert_eq!(first_error_line, 3);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(status(&t, "good.go"), ParseStatus::Ok);
}

#[gsttt_scenario::scenario(
    "go",
    "A syntax error inside a function keeps the declarations of the file"
)]
#[test]
fn a_syntax_error_inside_a_function_keeps_the_declarations_of_the_file() {
    let t = module(&[
        (
            "h.go",
            "package m\n\nvar Limit = 3\n\nfunc New() int { return new(f(1)) }\n\nfunc Run() { Helper() }\n",
        ),
        ("helper.go", "package m\n\nfunc Helper() {}\n"),
    ]);
    assert!(matches!(status(&t, "h.go"), ParseStatus::Recovered { .. }));
    assert_eq!(
        t.declared("h.go"),
        ["Limit L3-3: ", "New L5-5: f new", "Run L7-7: Helper"]
    );
    assert_eq!(t.exports("h.go"), ["Limit", "New", "Run"]);
    assert_eq!(t.uses("h.go", "Run"), "Helper");
    assert_eq!(
        t.imports("h.go"),
        [". [Helper,f,new] [Helper,f,new] -> helper.go"]
    );
}

#[gsttt_scenario::scenario("go", "A syntax error at the top level keeps nothing")]
#[test]
fn a_syntax_error_at_the_top_level_keeps_nothing() {
    let t = module(&[
        ("h.go", "package m\n\n)\n\nfunc Run() { Helper() }\n"),
        ("helper.go", "package m\n\nfunc Helper() {}\n"),
    ]);
    assert!(matches!(status(&t, "h.go"), ParseStatus::Recovered { .. }));
    let facts = &t.file("h.go").facts;
    assert!(facts.symbols.is_none() && facts.exports.is_none());
}

/// A file of `depth` nested parentheses.
fn nested(depth: usize, close: bool) -> String {
    let mut text = String::from("package m\n\nvar x = ");
    text.push_str(&"(".repeat(depth));
    text.push('1');
    if close {
        text.push_str(&")".repeat(depth));
    }
    text.push('\n');
    text
}

#[gsttt_scenario::scenario("go", "A file nested too deep does not take the index down")]
#[test]
fn a_file_nested_too_deep_does_not_take_the_index_down() {
    let deep = nested(100_000, true);
    let open = nested(100_000, false);
    let t = module(&[
        ("normal.go", "package m\n\nfunc Normal() {}\n"),
        ("deep.go", &deep),
        ("open.go", &open),
    ]);
    for path in ["deep.go", "open.go"] {
        assert!(
            matches!(
                status(&t, path),
                ParseStatus::Recovered { .. } | ParseStatus::Failed(_)
            ),
            "{path}: {:?}",
            status(&t, path)
        );
    }
    assert_eq!(status(&t, "normal.go"), ParseStatus::Ok);
    assert_eq!(t.exports("normal.go"), ["Normal"]);
}

/// A file just under the limit is read whole, on the stack of a default test thread.
#[test]
fn a_file_just_under_the_depth_limit_is_read() {
    let near = nested(1_500, true);
    let t = module(&[("near.go", &near)]);
    assert_eq!(
        status(&t, "near.go"),
        ParseStatus::Ok,
        "{:?}",
        status(&t, "near.go")
    );
    assert_eq!(t.exports("near.go"), ["x"]);
}

#[gsttt_scenario::scenario("go", "A method in a file of its own exports its type")]
#[test]
fn a_method_in_a_file_of_its_own_exports_its_type() {
    let t = module(&[
        ("service.go", "package m\n\ntype Service struct{}\n"),
        ("charge.go", "package m\n\nfunc (s *Service) Charge() {}\n"),
        (
            "get.go",
            "package m\n\ntype Box[T any] struct{}\n\nfunc (b Box[T]) Get() {}\n",
        ),
    ]);
    assert_eq!(t.exports("charge.go"), ["Service"]);
    assert_eq!(t.exports("get.go"), ["Box"]);
    // The type is still read from its package, so the method's file reaches the file that declares it.
    assert_eq!(
        t.imports("charge.go"),
        [". [Service] [Service] -> service.go"]
    );
}

#[gsttt_scenario::scenario(
    "go",
    "A method read through the first result of a constructor is that method"
)]
#[test]
fn a_method_read_through_the_first_result_of_a_constructor_is_that_method() {
    let t = module(&[
        (
            "h.go",
            "package m\n\nimport \"example.com/m/inv\"\n\nfunc ByTuple(cfg Cfg) {\n\tsvc, err := NewService(cfg)\n\t_ = err\n\tsvc.Charge()\n}\n\nfunc ByQualified() {\n\tsvc, _ := inv.NewService()\n\tsvc.Charge()\n}\n",
        ),
        (
            "inv/i.go",
            "package inv\n\ntype Service struct{}\n\nfunc NewService() (*Service, error) { return nil, nil }\n",
        ),
    ]);
    assert_eq!(t.uses("h.go", "ByTuple"), "Cfg NewService");
    assert_eq!(t.uses("h.go", "ByQualified"), "inv.NewService");
    assert_eq!(t.reads("h.go"), ["Charge"]);
}

#[gsttt_scenario::scenario("go", "A go.mod with no module line leaves the other projects")]
#[test]
fn a_go_mod_with_no_module_line_leaves_the_other_projects() {
    let t = tree(&[
        ("go.mod", "module example.com/m\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/m/util\"\n\nfunc main() { util.Do() }\n",
        ),
        ("util/util.go", "package util\n\nfunc Do() {}\n"),
        ("legacy/go.mod", "go 1.12\n"),
        ("legacy/a.go", "package legacy\n\nfunc A() { B() }\n"),
        ("legacy/b.go", "package legacy\n\nfunc B() {}\n"),
    ]);
    assert_eq!(t.roots(), ["", "legacy"]);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/m/util [Do] [util.Do] -> util/util.go"]
    );
    assert_eq!(t.imports("legacy/a.go"), [". [B] [B] -> legacy/b.go"]);
}

#[gsttt_scenario::scenario(
    "go",
    "An import of a package of a nested module reaches only the files that declare what is read"
)]
#[test]
fn an_import_of_a_package_of_a_nested_module_reaches_only_the_files_that_declare_what_is_read() {
    let t = tree(&[
        ("go.mod", "module example.com/m\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/tools/gen\"\n\nfunc main() { gen.Run() }\n",
        ),
        (
            "blank.go",
            "package main\n\nimport _ \"example.com/tools/gen\"\n",
        ),
        ("tools/go.mod", "module example.com/tools\n"),
        ("tools/gen/gen.go", "package gen\n\nfunc Run() {}\n"),
        ("tools/gen/other.go", "package gen\n\nfunc Other() {}\n"),
        (
            "tools/gen/boot.go",
            "package gen\n\nfunc init() { Other() }\n",
        ),
    ]);
    assert_eq!(
        t.imports("main.go"),
        ["example.com/tools/gen [Run] [gen.Run] -> tools/gen/boot.go + tools/gen/gen.go"]
    );
    assert_eq!(
        t.imports("blank.go"),
        [
            "example.com/tools/gen [] [] -> tools/gen/boot.go + tools/gen/gen.go + tools/gen/other.go"
        ]
    );
}

#[gsttt_scenario::scenario(
    "go",
    "A call into the standard library or a module does not make a file run on load"
)]
#[test]
fn a_call_into_the_standard_library_or_a_module_does_not_make_a_file_run_on_load() {
    let t = module(&[
        (
            "std.go",
            "package m\n\nimport (\n\t\"errors\"\n\t\"regexp\"\n)\n\nvar re = regexp.MustCompile(\"x\")\n\nvar failed = errors.New(\"y\")\n\nvar made = make([]int, 1)\n",
        ),
        (
            "own.go",
            "package m\n\nimport \"example.com/m/inv\"\n\nvar a = inv.Build()\n\nvar b = build()\n",
        ),
        ("inv/i.go", "package inv\n\nfunc Build() {}\n"),
        ("b.go", "package m\n\nfunc build() {}\n"),
        (
            "use.go",
            "package m\n\nimport \"example.com/m/inv\"\n\nfunc F() { inv.Build() }\n",
        ),
    ]);
    assert_eq!(t.on_load("std.go"), "");
    assert_eq!(t.on_load("own.go"), "build inv.Build");
    // `std.go` runs nothing as the package loads, so an import of `m` would not reach it for that.
    assert_eq!(
        t.imports("use.go"),
        ["example.com/m/inv [Build] [inv.Build] -> inv/i.go"]
    );
}

#[gsttt_scenario::scenario("go", "A file of another package of the directory is no sibling")]
#[test]
fn a_file_of_another_package_of_the_directory_is_no_sibling() {
    let t = module(&[
        ("real.go", "package lib\n\nfunc Real() {}\n"),
        (
            "gen.go",
            "//go:build ignore\n\npackage main\n\nfunc Helper() {}\n",
        ),
        (
            "ext_test.go",
            "package lib_test\n\nfunc Fixture() { Real() }\n",
        ),
        (
            "lib_test.go",
            "package lib\n\nfunc TestIt() { Real(); Helper(); Fixture() }\n",
        ),
    ]);
    assert_eq!(t.package("gen.go"), "-");
    assert_eq!(t.package("ext_test.go"), "lib_test");
    assert_eq!(t.package("real.go"), "lib");
    assert_eq!(
        t.imports("lib_test.go"),
        [". [Fixture,Helper,Real] [Fixture,Helper,Real] -> real.go"]
    );
    // The external test package sees the package through an import only.
    assert_eq!(t.imports("ext_test.go"), [". [Real] [Real] -> "]);
}

#[gsttt_scenario::scenario("go", "A sibling read past a syntax error may declare any name")]
#[test]
fn a_sibling_read_past_a_syntax_error_may_declare_any_name() {
    let t = module(&[
        ("a.go", "package m\n\nfunc A() { Helper() }\n"),
        ("broken.go", "package m\n\nfunc Broken( {\n"),
        ("fine.go", "package m\n\nfunc Fine() {}\n"),
    ]);
    assert_eq!(t.imports("a.go"), [". [Helper] [Helper] -> broken.go"]);
}

#[gsttt_scenario::scenario("go", "A method called from the file that declares its type")]
#[test]
fn a_method_called_from_the_file_that_declares_its_type() {
    let t = module(&[
        (
            "command.go",
            "package m\n\ntype Command struct{ Use string }\n\nfunc (c *Command) Own() {}\n\nfunc (c *Command) Run() { c.Own(); c.Init(); _ = c.Use }\n",
        ),
        ("init.go", "package m\n\nfunc (c *Command) Init() {}\n"),
        (
            "only.go",
            "package m\n\nfunc (c *Command) Only(d *Command) { d.Init() }\n",
        ),
    ]);
    // `Init` is declared elsewhere, so the file reaches the siblings that may declare it; `Own` and `Use` are its own.
    assert_eq!(
        t.imports("command.go"),
        [". [Command] [Command] -> init.go + only.go"]
    );
    // A file that only has receivers of the type reaches the siblings too.
    assert_eq!(
        t.imports("only.go"),
        [". [Command] [Command] -> command.go + init.go"]
    );
}

#[gsttt_scenario::scenario("go", "A use of a type is no use of its members")]
#[test]
fn a_use_of_a_type_is_no_use_of_its_members() {
    let t = module(&[
        ("t.go", "package m\n\ntype Command struct{}\n"),
        (
            "u.go",
            "package m\n\ntype byName []*Command\n\nvar hook func(*Command)\n\nfunc Make() { _ = &Command{} }\n",
        ),
    ]);
    assert_eq!(t.uses("u.go", "byName"), "Command");
    assert_eq!(t.uses("u.go", "hook"), "Command");
    assert_eq!(t.uses("u.go", "Make"), "Command");
}

/// A package variable of a sibling is reached through its name even when an import is read by a name its path does not end in.
#[test]
fn a_sibling_variable_is_reached_when_a_named_import_finds_no_selector() {
    let t = module(&[
        ("lib/x.go", "package other\n\nfunc Foo() {}\n"),
        ("reg.go", "package main\n\nvar registry = 1\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/m/lib\"\n\nfunc main() { other.Foo(); registry.Add(1) }\n",
        ),
    ]);
    assert!(
        t.imports("main.go")
            .contains(&". [other,registry] [other,registry] -> reg.go".to_owned()),
        "{:?}",
        t.imports("main.go")
    );
}

#[gsttt_scenario::scenario("go", "A router held in a field serves its routes")]
#[test]
fn a_router_held_in_a_field_serves_its_routes() {
    let t = module(&[(
        "s.go",
        "package main\n\nimport \"net/http\"\n\ntype S struct{ mux *http.ServeMux }\n\nfunc (s *S) F() {\n\ts.mux.HandleFunc(\"GET /a\", h)\n\ts.a.b.Handle(\"/b\", h)\n}\n",
    )]);
    assert_eq!(
        t.contracts("s.go"),
        ["serves http GET /a@8", "serves http * /b@9"]
    );
}

#[gsttt_scenario::scenario("go", "A Handle whose first argument is a typed value is no route")]
#[test]
fn a_handle_whose_first_argument_is_a_typed_value_is_no_route() {
    let t = module(&[(
        "log.go",
        "package main\n\nimport \"context\"\n\nfunc (h *H) F(ctx context.Context, rec Record) {\n\th.inner.Handle(ctx, rec)\n}\n\nfunc (h *H) G(pattern string) {\n\th.mux.Handle(pattern, nil)\n}\n",
    )]);
    assert_eq!(t.contracts("log.go"), Vec::<String>::new());
    // Only the call that passes a bare string-typed name is a blind spot.
    assert_eq!(t.unresolved_lines("log.go"), [10]);
}

/// A constant built as `a + a` over twenty levels is read in bounded work and is not text.
#[test]
fn a_constant_built_by_doubling_is_read_in_bounded_work() {
    use std::fmt::Write as _;
    let mut text = String::from("package m\n\nimport \"net/http\"\n\nconst a0 = \"x\"\n");
    for level in 1..=20 {
        let prev = level - 1;
        writeln!(text, "const a{level} = a{prev} + a{prev}").unwrap();
    }
    text.push_str("\nfunc F() { http.Get(a20) }\n");
    let t = module(&[("f.go", &text)]);
    assert_eq!(t.contracts("f.go"), Vec::<String>::new());
    assert_eq!(t.unresolved_lines("f.go").len(), 1);
}

#[gsttt_scenario::scenario("go", "A function takes the members of its first result")]
#[test]
fn a_function_takes_the_members_of_its_first_result() {
    let t = module(&[(
        "a.go",
        "package m

import \"pkg\"

type Mux struct{}

func NewRouter() *Mux { return nil }

func F() (*Mux, error) { return nil, nil }

func L() []*Mux { return nil }

func Q() *pkg.T { return nil }

func E() error { return nil }

func G[T any]() T { var t T; return t }

func (m *Mux) Self() *Mux { return m }
",
    )]);
    assert_eq!(
        t.heirs("a.go"),
        ["F Mux", "L Mux", "NewRouter Mux", "Q pkg.T"]
    );
}

#[gsttt_scenario::scenario("go", "An embedded type gives its members")]
#[test]
fn an_embedded_type_gives_its_members() {
    let t = module(&[(
        "a.go",
        "package m

import \"pkg\"

type Engine struct {
	RouterGroup
	*pkg.Base
	name string
}

type A interface {
	B
	pkg.C
	Do()
}

type Number interface {
	~int | string
}

type Plain interface {
	int
}
",
    )]);
    assert_eq!(
        t.heirs("a.go"),
        ["A B", "A pkg.C", "Engine RouterGroup", "Engine pkg.Base"]
    );
}

#[gsttt_scenario::scenario("go", "An alias takes the members of its type")]
#[test]
fn an_alias_takes_the_members_of_its_type() {
    let t = module(&[(
        "a.go",
        "package m

type T struct{}

type A = T

type D T
",
    )]);
    assert_eq!(t.heirs("a.go"), ["A T"]);
}

const STORE_IFACE: &str = "package svc

type Store interface {
	Get(id string) (*User, error)
}
";

const USE: &str = "package svc

func Run(s Store) {
	s.Get(\"x\")
}
";

#[gsttt_scenario::scenario("go", "A method is reached through an interface its type satisfies")]
#[test]
fn a_method_is_reached_through_an_interface_its_type_satisfies() {
    let t = module(&[
        (
            "store/pg.go",
            "package store

type PG struct{}

func (p *PG) Get(id string) (*User, error) { return nil, nil }
",
        ),
        ("svc/svc.go", STORE_IFACE),
        ("svc/use.go", USE),
    ]);
    assert_eq!(t.overrides(), ["store/pg.go: PG -> svc/svc.go Store"]);
}

#[gsttt_scenario::scenario("go", "A method of another signature is no implementation")]
#[test]
fn a_method_of_another_signature_is_no_implementation() {
    let t = module(&[
        (
            "store/cache.go",
            "package store

type Cache struct{}

func (c *Cache) Get(key int) string { return \"\" }
",
        ),
        (
            "store/other.go",
            "package store

type Pkg struct{}

func (c *Pkg) Get(id string, more ...string) (*User, error) { return nil, nil }
",
        ),
        ("svc/svc.go", STORE_IFACE),
        ("svc/use.go", USE),
    ]);
    assert_eq!(t.overrides(), Vec::<String>::new());
}

#[gsttt_scenario::scenario("go", "A type needs every method of the interface")]
#[test]
fn a_type_needs_every_method_of_the_interface() {
    let t = module(&[
        (
            "store/one.go",
            "package store

type One struct{}

func (One) Get(id string) int { return 0 }
",
        ),
        (
            "store/two.go",
            "package store

type Two struct{}

func (Two) Get(id string) int { return 0 }

func (Two) Put(id string) {}
",
        ),
        (
            "svc/svc.go",
            "package svc

type Store interface {
	Get(id string) int
	Put(name string)
}
",
        ),
    ]);
    assert_eq!(t.overrides(), ["store/two.go: Two -> svc/svc.go Store"]);
}

#[gsttt_scenario::scenario("go", "The methods of a package count together")]
#[test]
fn the_methods_of_a_package_count_together() {
    let t = module(&[
        ("store/pg.go", "package store\n\ntype PG struct{}\n"),
        (
            "store/get.go",
            "package store\n\nfunc (PG) Get(id string) int { return 0 }\n",
        ),
        (
            "store/put.go",
            "package store\n\nfunc (PG) Put(name string) {}\n",
        ),
        (
            "svc/svc.go",
            "package svc

type Store interface {
	Get(a string) int
	Put(b string)
}
",
        ),
    ]);
    assert_eq!(
        t.overrides(),
        [
            "store/get.go: PG -> svc/svc.go Store",
            "store/put.go: PG -> svc/svc.go Store",
        ]
    );
}

#[gsttt_scenario::scenario("go", "An embedded type lends its methods")]
#[test]
fn an_embedded_type_lends_its_methods() {
    let t = module(&[
        (
            "lib/base.go",
            "package lib\n\ntype Base struct{}\n\nfunc (Base) Get(id string) int { return 0 }\n",
        ),
        (
            "app/local.go",
            "package app

type Local struct{}

func (Local) Get(id string) int { return 0 }

type Wrap struct {
	Local
}

func (Wrap) Put(name string) {}
",
        ),
        (
            "app/t.go",
            "package app

import \"example.com/m/lib\"

type T struct {
	lib.Base
}

func (T) Put(name string) {}

type Store interface {
	Get(id string) int
	Put(name string)
}
",
        ),
    ]);
    assert_eq!(
        t.overrides(),
        [
            "app/local.go: Local -> app/t.go Store",
            "app/local.go: Wrap -> app/t.go Store",
            "app/t.go: T -> app/t.go Store",
            "lib/base.go: Base -> app/t.go Store",
        ]
    );
}

#[gsttt_scenario::scenario("go", "A type that lends its methods is a head in its own files")]
#[test]
fn a_type_that_lends_its_methods_is_a_head_in_its_own_files() {
    let t = module(&[
        (
            "jobs/base.go",
            "package jobs\n\ntype Base struct{}\n\nfunc (Base) Name() string { return \"\" }\n",
        ),
        (
            "jobs/job.go",
            "package jobs\n\ntype Job struct {\n\tBase\n}\n\nfunc (Job) Run() {}\n",
        ),
        (
            "jobs/task.go",
            "package jobs\n\ntype Task interface {\n\tName() string\n\tRun()\n}\n",
        ),
        (
            "use/use.go",
            "package use\n\nimport \"example.com/m/jobs\"\n\nfunc Use(t jobs.Task) string { t.Run(); return t.Name() }\n",
        ),
    ]);
    assert_eq!(
        t.overrides(),
        [
            "jobs/base.go: Base -> jobs/task.go Task",
            "jobs/job.go: Job -> jobs/task.go Task",
        ]
    );
}

#[gsttt_scenario::scenario("go", "Names inside a function type do not count")]
#[test]
fn names_inside_a_function_type_do_not_count() {
    let t = module(&[
        (
            "mem/tree.go",
            "package mem

type Tree struct{}

func (Tree) Walk(fn func(path string, n int) error) error { return nil }
func (Tree) Pick(f func(a, b int) (int, error), g []func(x string) bool) {}
func (Tree) Half(fn func(path string, n int) error) error { return nil }
",
        ),
        (
            "store/store.go",
            "package store

type Walker interface {
	Walk(visit func(p string, count int) error) error
	Pick(f func(c, d int) (int, error), h []func(y string) bool)
}

type Half interface {
	Half(visit func(p string) error) error
}
",
        ),
    ]);
    assert_eq!(
        t.overrides(),
        ["mem/tree.go: Tree -> store/store.go Walker"]
    );
}

#[gsttt_scenario::scenario("go", "An interface that embeds a generic one is a head")]
#[test]
fn an_interface_that_embeds_a_generic_one_is_a_head() {
    let t = module(&[
        (
            "mem/is.go",
            "package mem

type IS struct{}

func (IS) Get(k string) int { return 0 }
func (IS) Size() int        { return 0 }
",
        ),
        (
            "store/store.go",
            "package store

type Getter[T any] interface {
	Get(k string) T
}

type IntStore interface {
	Getter[int]
	Size() int
}
",
        ),
    ]);
    assert_eq!(
        t.overrides(),
        [
            "mem/is.go: IS -> store/store.go Getter",
            "mem/is.go: IS -> store/store.go IntStore",
            "store/store.go: IntStore -> store/store.go Getter",
        ]
    );
}

#[gsttt_scenario::scenario("go", "An interface with a union is no head")]
#[test]
fn an_interface_with_a_union_is_no_head() {
    let t = module(&[(
        "a.go",
        "package m

type Num interface {
	~int | ~string
	Get() int
}

type Only interface {
	int
	Get() int
}

type T struct{}

func (T) Get() int { return 0 }
",
    )]);
    assert_eq!(t.overrides(), Vec::<String>::new());
}

#[gsttt_scenario::scenario("go", "An interface that knows no method is no head")]
#[test]
fn an_interface_that_knows_no_method_is_no_head() {
    let t = module(&[(
        "a.go",
        "package m

type Nothing interface{}

type Also interface {
	Nothing
}

type T struct{}

func (T) Get() int { return 0 }
",
    )]);
    assert_eq!(t.overrides(), Vec::<String>::new());
}

#[gsttt_scenario::scenario("go", "Package clause separates a test package")]
#[test]
fn a_package_clause_separates_a_test_package() {
    let t = module(&[
        (
            "foo/t.go",
            "package foo\n\ntype T struct{}\n\nfunc (T) Get() int { return 0 }\n",
        ),
        (
            "foo/t_test.go",
            "package foo_test\n\ntype T struct{}\n\nfunc (T) Get() string { return \"\" }\n",
        ),
        (
            "other/i.go",
            "package other\n\ntype I interface {\n\tGet() int\n}\n",
        ),
    ]);
    assert_eq!(t.overrides(), ["foo/t.go: T -> other/i.go I"]);
}

#[gsttt_scenario::scenario("go", "A package variable takes the members of what it holds")]
#[test]
fn a_package_variable_takes_the_members_of_what_it_holds() {
    let t = module(&[(
        "a.go",
        "package m

import \"pkg\"

type T struct{}

func F() *T { return nil }

var A T
var B = T{}
var C = &T{}
var D = new(T)
var E = F()
var G = pkg.F()
var _ = F()

const K = T(1)
",
    )]);
    assert_eq!(
        t.heirs("a.go"),
        ["A T", "B T", "C T", "D T", "E F", "F T", "G pkg.F"]
    );
}

#[gsttt_scenario::scenario("go", "A value a call returns is read")]
#[test]
fn a_value_a_call_returns_is_read() {
    let t = module(&[(
        "a.go",
        "package m

import \"github.com/acme/pkg\"

func Run() {
	x := pkg.F()
	x.M()
	pkg.F().N()
}
",
    )]);
    assert_eq!(t.uses("a.go", "Run"), "pkg.F");
    assert_eq!(t.reads("a.go"), ["M", "N"]);
}

#[gsttt_scenario::scenario("go", "A verb on a gin engine serves its route")]
#[test]
fn a_verb_on_a_gin_engine_serves_its_route() {
    let t = index("routers-gin");
    let contracts = t.contracts("routes.go");
    assert_eq!(contracts[0], "serves http GET /health@6");
    // The field's type is declared in another file, so its type is unknown here.
    assert_eq!(contracts[1], "serves http POST /orders@7");
    // `probe.GET("/health")` has one argument: a request of a test client.
    assert!(!contracts.iter().any(|fact| fact.ends_with("@14")));
    assert_eq!(t.unresolved_lines("routes.go"), Vec::<u32>::new());
}

#[gsttt_scenario::scenario("go", "A verb given as text serves its route")]
#[test]
fn a_verb_given_as_text_serves_its_route() {
    let t = index("routers-gin");
    let contracts = t.contracts("handle.go");
    assert!(
        contracts.contains(&"serves http GET /handled@6".to_owned()),
        "{contracts:?}"
    );
    assert!(
        !contracts.iter().any(|fact| fact.contains("http * GET")),
        "{contracts:?}"
    );
    assert_eq!(t.unresolved_lines("handle.go"), Vec::<u32>::new());
    for fixture in ["routers-echo", "routers-fiber"] {
        let contracts = index(fixture).contracts("add.go");
        assert_eq!(contracts, ["serves http GET /added@6"], "{fixture}");
    }
}

#[gsttt_scenario::scenario("go", "A group adds its prefix")]
#[test]
fn a_group_adds_its_prefix() {
    let gin = index("routers-gin");
    assert_eq!(
        gin.contracts("routes.go")[2..],
        [
            "serves http GET /v1/users/{}@9",
            "serves http GET /v1/files/{}@10",
            "serves http DELETE /v1/users/{}@12",
            "serves http * /v1/users/ping@13"
        ]
    );
    assert_eq!(
        index("routers-echo").contracts("routes.go"),
        [
            "serves http GET /health@7",
            "serves http GET /api/items/{}@9",
            "serves http POST /api/v2/items@11"
        ]
    );
    assert_eq!(
        index("routers-fiber").contracts("routes.go"),
        [
            "serves http GET /health@7",
            "serves http GET /api/users/{}@9",
            "serves http POST /api/v2/users@11",
            "serves http GET /admin/stats@13",
            "serves http GET /shared@22",
            "serves http POST /todo/create@24"
        ]
    );
}

#[gsttt_scenario::scenario("go", "A chi verb on a router it can see serves its route")]
#[test]
fn a_chi_verb_on_a_router_it_can_see_serves_its_route() {
    let t = index("routers-chi");
    let contracts = t.contracts("routes.go");
    assert!(contracts.contains(&"serves http GET /ping@11".to_owned()));
    assert!(contracts.contains(&"serves http PUT /typed@27".to_owned()));
    assert!(contracts.contains(&"serves http PUT /things@19".to_owned()));
}

#[gsttt_scenario::scenario("go", "A chi closure adds its prefix")]
#[test]
fn a_chi_closure_adds_its_prefix() {
    let t = index("routers-chi");
    let contracts = t.contracts("routes.go");
    // `Route` adds its prefix, `Group` adds none, and `With` is the router it wraps.
    for fact in [
        "serves http GET /v1/users/{}@13",
        "serves http POST /v1/users@15",
        "serves http DELETE /v1/users/{}@17",
    ] {
        assert!(
            contracts.contains(&fact.to_owned()),
            "{fact} in {contracts:?}"
        );
    }
}

#[gsttt_scenario::scenario("go", "A Get on what is no router is no route")]
#[test]
fn a_get_on_what_is_no_router_is_no_route() {
    let t = index("routers-chi");
    let contracts = t.contracts("routes.go");
    assert!(
        !contracts
            .iter()
            .any(|fact| fact.ends_with("@20") || fact.ends_with("@22"))
    );
    assert_eq!(t.unresolved_lines("routes.go"), Vec::<u32>::new());
    let fiber = index("routers-fiber");
    assert!(
        !fiber
            .contracts("routes.go")
            .iter()
            .any(|fact| fact.ends_with("@15"))
    );
    assert_eq!(fiber.unresolved_lines("routes.go"), Vec::<u32>::new());
}

#[gsttt_scenario::scenario("go", "A gorilla sub-router adds its prefix")]
#[test]
fn a_gorilla_sub_router_adds_its_prefix() {
    let t = index("routers-gorilla");
    assert_eq!(
        t.contracts("routes.go"),
        [
            "serves http * /health@7",
            "serves http GET /api/users/{}@9",
            "serves http POST /api/users/{}@9",
            "serves http PUT /api/orders@10",
            "serves http * /api/v1/files@12"
        ]
    );
}

#[gsttt_scenario::scenario("go", "A group whose prefix is no text serves no route")]
#[test]
fn a_group_whose_prefix_is_no_text_serves_no_route() {
    let t = tree(&[
        (
            "go.mod",
            "module example.com/m\n\ngo 1.22\n\nrequire github.com/gin-gonic/gin v1.9.1\n",
        ),
        (
            "r.go",
            "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc f(r *gin.Engine, p string) {\n\tg := r.Group(p)\n\tg.GET(\"/x\", h)\n}\n",
        ),
    ]);
    assert_eq!(t.contracts("r.go"), Vec::<String>::new());
    assert_eq!(t.unresolved_lines("r.go"), [7]);
}

#[gsttt_scenario::scenario(
    "go",
    "A pack reads nothing where go.mod does not require its framework"
)]
#[test]
fn a_pack_reads_nothing_where_go_mod_does_not_require_its_framework() {
    let t = index("routers-unrequired");
    assert_eq!(t.contracts("routes.go"), Vec::<String>::new());
    assert_eq!(t.unresolved_lines("routes.go"), Vec::<u32>::new());
}

/// The facts of the router fixtures, for review.
#[test]
fn the_router_fixtures_facts() {
    let facts: serde_json::Map<String, Value> = [
        "routers-gin",
        "routers-echo",
        "routers-fiber",
        "routers-chi",
        "routers-gorilla",
        "routers-unrequired",
    ]
    .into_iter()
    .map(|name| {
        let t = index(name);
        let value = serde_json::json!({
            "contracts": t.contracts("routes.go"),
            "unresolved": t.unresolved_lines("routes.go"),
        });
        (name.to_owned(), value)
    })
    .collect();
    insta::assert_json_snapshot!("the_router_fixtures_facts", facts);
}

/// What the Go project of a one-module tree gives for `text` as the code of the file at `path`.
fn code_of(path: &str, text: &str) -> Option<String> {
    let t = module(&[(path, text)]);
    let projects = GO
        .open(&t.repo, &Value::Null, &builtin_packs(PACKS).unwrap())
        .unwrap();
    let path = RepoPath::normalize(path).unwrap();
    projects[0].code(&SourceFile {
        id: FileId::from_raw(0),
        path: &path,
        text,
    })
}

/// The lines of `code` without the space around them, the way two versions of a file are compared.
fn trimmed(code: &str) -> Vec<&str> {
    code.lines().map(str::trim).collect()
}

/// The code of a Go file whose body is `body`, below a package clause.
fn body_code(body: &str) -> Vec<String> {
    let code = code_of("a.go", &format!("package a\n\n{body}\n")).unwrap();
    trimmed(&code).into_iter().map(str::to_owned).collect()
}

#[gsttt_scenario::scenario("go", "A comment or an import does not change the code")]
#[test]
fn a_comment_or_an_import_does_not_change_the_code() {
    let before = code_of(
        "a.go",
        "package a\n\nimport \"os\"\n\n// note\nvar x = 1 // tail\n",
    )
    .unwrap();
    let after = code_of(
        "a.go",
        "package a\n\nimport \"fmt\"\n\n/* other */\nvar x = 1\n",
    )
    .unwrap();
    assert_eq!(trimmed(&before), trimmed(&after));
    assert_eq!(before.lines().count(), after.lines().count());
}

#[gsttt_scenario::scenario("go", "A realigned block does not change the code")]
#[test]
fn a_realigned_block_does_not_change_the_code() {
    assert_eq!(
        body_code("type T struct {\n\ta    int\n\tbb   string\n}"),
        body_code("type T struct {\n\ta int\n\tbb string\n}")
    );
    assert_eq!(
        body_code("var m = map[string]int{\n\t\"a\":   1,\n\t\"bbb\": 2,\n}"),
        body_code("var m = map[string]int{\n\t\"a\": 1,\n\t\"bbb\": 2,\n}")
    );
    assert_eq!(
        body_code("const (\n\tA   = 1\n\tBBB = 2\n)"),
        body_code("const (\n\tA = 1\n\tBBB = 2\n)")
    );
    assert_ne!(
        body_code("type T struct {\n\ta int\n}"),
        body_code("type T struct {\n\ta int\n\tb int\n}")
    );
}

#[gsttt_scenario::scenario("go", "Whitespace in a string is part of the code")]
#[test]
fn whitespace_in_a_string_is_part_of_the_code() {
    assert_ne!(body_code("var s = \"a  b\""), body_code("var s = \"a b\""));
    let raw = body_code("var s = `a\n  b\n\n\tc`");
    assert_eq!(
        raw,
        [
            "package a",
            "",
            "var s = `a\x03",
            "\x01\x01b\x03",
            "\x03",
            "\x02c`"
        ]
    );
    assert_ne!(body_code("var r = ' '"), body_code("var r = '\t'"));
}

#[gsttt_scenario::scenario("go", "Tokens that a space keeps apart stay apart")]
#[test]
fn tokens_that_a_space_keeps_apart_stay_apart() {
    assert_ne!(body_code("var x = a &^ b"), body_code("var x = a & ^b"));
    assert_ne!(body_code("var x = a <- b"), body_code("var x = a < -b"));
}

#[gsttt_scenario::scenario("go", "A directive is part of the code")]
#[test]
fn a_directive_is_part_of_the_code() {
    for (one, two) in [
        ("//go:build linux", "//go:build darwin"),
        ("//go:embed a.txt", "//go:embed b.txt"),
        ("// +build linux", "// +build darwin"),
    ] {
        assert_ne!(
            body_code(&format!("{one}\nvar x = 1")),
            body_code(&format!("{two}\nvar x = 1")),
            "{one}"
        );
    }
}

#[gsttt_scenario::scenario("go", "The preamble of cgo is part of the code")]
#[test]
fn the_preamble_of_cgo_is_part_of_the_code() {
    let cgo = |c: &str| {
        code_of(
            "a.go",
            &format!("package a\n\n// {c}\nimport \"C\"\n\nvar x = 1\n"),
        )
        .unwrap()
    };
    assert_ne!(
        trimmed(&cgo("#include <a.h>")),
        trimmed(&cgo("#include <b.h>"))
    );
}

#[gsttt_scenario::scenario("go", "A file with a syntax error has no code")]
#[test]
fn a_file_with_a_syntax_error_has_no_code() {
    assert_eq!(code_of("a.go", "package a\n\nfunc f( {\n"), None);
    assert_eq!(code_of("go.mod", "module example.com/m\n\ngo 1.22\n"), None);
}

/// The facts of the `service` fixture, for review: what each file declares, imports and finds.
#[test]
fn the_service_fixture_facts() {
    let t = index("service");
    let files: serde_json::Map<String, Value> = [
        "service.go",
        "db.go",
        "invoice/header.go",
        "invoice/footer.go",
        "invoice/line.go",
        "billing/init.go",
    ]
    .into_iter()
    .map(|path| {
        let value = serde_json::json!({
            "declared": t.declared(path),
            "on_load": t.on_load(path),
            "exports": t.exports(path),
            "imports": t.imports(path),
            "contracts": t.contracts(path),
            "embedded": t.embedded(path),
        });
        (path.to_owned(), value)
    })
    .collect();
    assert!(!t.claims("vendor/x/x.go") && !t.claims("testdata/t.go"));
    insta::assert_json_snapshot!("the_service_fixture_facts", files);
}

// Positions.

#[gsttt_scenario::scenario("go", "A use keeps the line it is written on")]
#[test]
fn a_use_keeps_the_line_it_is_written_on() {
    let t = module(&[(
        "u.go",
        "package m\n\nfunc Run(s Store) {\n\tHelper()\n\ts.\n\t\tSave()\n}\n\nvar loaded = Prepare()\n",
    )]);
    let symbols = t.file("u.go").facts.symbols.as_ref().expect("symbols");
    let lines = |uses: &[Use]| -> Vec<(String, Line)> {
        uses.iter()
            .map(|used| (used.name.clone(), used.line))
            .collect()
    };
    let run = symbols
        .declared
        .iter()
        .find(|declared| declared.name == "Run")
        .expect("Run is declared");
    let found = lines(&run.uses);
    let line_of = |name: &str| {
        found
            .iter()
            .find(|(used, _)| used == name)
            .map(|(_, line)| *line)
    };
    assert_eq!(line_of("Helper"), Some(4), "{found:?}");
    // The receiver stands on line 5 and the method it calls on line 6.
    assert_eq!(line_of("Store.Save"), Some(6), "{found:?}");
    assert_eq!(lines(&symbols.on_load), [("Prepare".to_owned(), 9)]);
}
