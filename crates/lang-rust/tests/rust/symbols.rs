//! The declarations of a Rust file: what it declares, what each declaration uses, and what a walk by name makes of it. The trees are a few lines each, written to a temporary directory.

use gsttt_scenario::scenario;

use crate::support::crate_of;

#[scenario("rust", "A name gets its callers and not every importer")]
#[test]
fn a_name_gets_its_callers_and_not_every_importer() {
    let tree = crate_of(
        &["a", "b", "c"],
        &[
            ("src/a.rs", "pub fn foo() {}\npub fn bar() {}\n"),
            (
                "src/b.rs",
                "use crate::a::foo;\n\npub fn run() {\n    foo();\n}\n",
            ),
            (
                "src/c.rs",
                "use crate::a::bar;\n\npub fn stop() {\n    bar();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.declared("src/a.rs"), ["foo L1-1", "bar L2-2"]);
    assert_eq!(tree.reached("src/a.rs", &["foo"], None), ["src/b.rs@1"]);
    assert_eq!(tree.reached("src/a.rs", &["bar"], None), ["src/c.rs@1"]);
}

#[scenario("rust", "A path in code is a use of the member it names")]
#[test]
fn a_path_in_code_is_a_use_of_the_member_it_names() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct Foo {\n    pub n: u8,\n}\n\nimpl Foo {\n    pub fn bar() -> Self {\n        Self::baz()\n    }\n\n    pub fn baz() -> Self {\n        Foo { n: 0 }\n    }\n\n    pub fn run(&self) {\n        self.baz_ref();\n    }\n\n    fn baz_ref(&self) {}\n}\n\npub fn make() -> Foo {\n    Foo::bar()\n}\n",
        )],
    );

    assert_eq!(tree.uses("src/a.rs", "Foo.bar"), ["Foo", "Foo.baz"]);
    assert_eq!(tree.uses("src/a.rs", "Foo.run"), ["Foo.baz_ref"]);
    assert_eq!(tree.uses("src/a.rs", "make"), ["Foo", "Foo.bar"]);
}

#[scenario(
    "rust",
    "A method called on a value of an unknown type counts by its name"
)]
#[test]
fn a_method_called_on_a_value_of_an_unknown_type_counts_by_its_name() {
    let tree = crate_of(
        &["a", "b"],
        &[
            (
                "src/a.rs",
                "pub struct Runner;\n\nimpl Runner {\n    pub fn start(&self) {}\n    pub fn stop(&self) {}\n}\n",
            ),
            (
                "src/b.rs",
                "use crate::a::Runner;\n\npub fn go<T>(value: T) {\n    value.start();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.uses("src/b.rs", "go"), [".start"]);
    assert_eq!(tree.reads("src/b.rs"), ["start"]);
    assert_eq!(
        tree.reached("src/a.rs", &["Runner.start"], None),
        ["src/b.rs@1"]
    );
    assert_eq!(
        tree.reached("src/a.rs", &["Runner.stop"], None),
        Vec::<String>::new()
    );
}

#[scenario("rust", "A receiver with a written type names its type")]
#[test]
fn a_receiver_with_a_written_type_names_its_type() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct Cache;\n\nimpl Cache {\n    pub fn get(&self) {}\n}\n\npub struct Holder {\n    cache: Cache,\n}\n\nimpl Holder {\n    pub fn read(&self) {\n        self.cache.get();\n    }\n}\n\npub fn typed(cache: &Cache) {\n    cache.get();\n}\n",
        )],
    );

    assert_eq!(
        tree.uses("src/a.rs", "Holder.read"),
        ["Cache.get", "Holder", "Holder.cache"]
    );
    assert_eq!(tree.uses("src/a.rs", "typed"), ["Cache", "Cache.get"]);
    // No member is read on a value of an unknown type; the only read is the pair, which names two members and no class.
    assert_eq!(tree.reads("src/a.rs"), ["cache.get"]);
}

#[scenario("rust", "A name bound by a pattern is no use")]
#[test]
fn a_name_bound_by_a_pattern_is_no_use() {
    let tree = crate_of(
        &["a", "b"],
        &[
            ("src/a.rs", "pub fn foo() {}\n"),
            (
                "src/b.rs",
                "use crate::a::foo;\n\npub fn shadow(foo: ()) {\n    let _ = foo;\n}\n\npub fn call() {\n    foo();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.uses("src/b.rs", "shadow"), Vec::<&str>::new());
    assert_eq!(tree.uses("src/b.rs", "call"), ["foo"]);
    assert_eq!(tree.reached("src/a.rs", &["foo"], None), ["src/b.rs@1"]);
}

#[scenario("rust", "A glob import passes the names on")]
#[test]
fn a_glob_import_passes_the_names_on() {
    let tree = crate_of(
        &["a", "b", "c"],
        &[
            ("src/a.rs", "pub fn foo() {}\npub fn bar() {}\n"),
            (
                "src/b.rs",
                "use crate::a::*;\n\npub fn run() {\n    foo();\n}\n",
            ),
            (
                "src/c.rs",
                "use crate::a::*;\n\npub fn stop() {\n    bar();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.imports("src/b.rs"), ["crate::a [*] [*]"]);
    assert_eq!(tree.reached("src/a.rs", &["foo"], None), ["src/b.rs@1"]);
    assert_eq!(tree.reached("src/a.rs", &["bar"], None), ["src/c.rs@1"]);
}

#[scenario("rust", "A name a file declares beats the one a glob brings in")]
#[test]
fn a_name_a_file_declares_beats_the_one_a_glob_brings_in() {
    let tree = crate_of(
        &["a", "b"],
        &[
            ("src/a.rs", "pub fn foo() {}\n"),
            (
                "src/b.rs",
                "use crate::a::*;\n\nfn foo() {}\n\npub fn run() {\n    foo();\n}\n",
            ),
        ],
    );

    assert_eq!(
        tree.reached("src/a.rs", &["foo"], None),
        Vec::<String>::new()
    );
}

#[scenario("rust", "A pub use glob hands out every name of the module")]
#[test]
fn a_pub_use_glob_hands_out_every_name_of_the_module() {
    let tree = crate_of(
        &["a", "hub", "user"],
        &[
            ("src/a.rs", "pub fn foo() {}\npub fn bar() {}\n"),
            ("src/hub.rs", "pub use crate::a::*;\n"),
            (
                "src/user.rs",
                "use crate::hub::foo;\n\npub fn run() {\n    foo();\n}\n",
            ),
        ],
    );

    assert_eq!(
        tree.imports("src/hub.rs"),
        ["crate::a [*] [*]", "crate::a [*] []"]
    );
    assert_eq!(
        tree.reached("src/a.rs", &["foo"], Some(2)),
        ["src/user.rs@1"]
    );
    assert_eq!(
        tree.reached("src/a.rs", &["bar"], Some(2)),
        Vec::<String>::new()
    );
}

#[scenario("rust", "A glob of a parent passes on what a glob of it brings in")]
#[test]
fn a_glob_of_a_parent_passes_on_what_a_glob_of_it_brings_in() {
    let tree = crate_of(
        &["error", "assets"],
        &[
            ("src/error.rs", "pub type Failure = u8;\n"),
            (
                "src/assets.rs",
                "use crate::error::*;\n\nmod theme;\n\npub fn load() -> Failure {\n    0\n}\n",
            ),
            (
                "src/assets/theme.rs",
                "use super::*;\n\npub fn read() -> Failure {\n    0\n}\n",
            ),
        ],
    );

    assert_eq!(
        tree.reached("src/error.rs", &["Failure"], None),
        ["src/assets.rs@1", "src/assets/theme.rs@1"]
    );
}

#[scenario("rust", "A pub use hands out the name it takes")]
#[test]
fn a_pub_use_hands_out_the_name_it_takes() {
    let tree = crate_of(
        &["a", "hub"],
        &[
            ("src/a.rs", "pub fn foo() {}\n"),
            (
                "src/hub.rs",
                "pub use crate::a::foo as renamed;\nuse crate::a::foo as private;\n",
            ),
        ],
    );

    assert_eq!(tree.exports("src/hub.rs"), ["renamed"]);
    assert_eq!(
        tree.imports("src/hub.rs"),
        [
            "crate::a::foo [foo] [renamed]",
            "crate::a::foo [foo] [private]"
        ]
    );
}

#[scenario("rust", "A variant imported by its path names its enum and itself")]
#[test]
fn a_variant_imported_by_its_path_names_its_enum_and_itself() {
    let tree = crate_of(
        &["a", "b"],
        &[
            ("src/a.rs", "pub enum Color {\n    Red,\n    Blue,\n}\n"),
            (
                "src/b.rs",
                "use crate::a::Color::Red;\n\npub fn paint() {\n    let _ = Red;\n}\n",
            ),
        ],
    );

    assert_eq!(
        tree.imports("src/b.rs"),
        ["crate::a::Color::Red [Color] [Red]"]
    );
    assert_eq!(
        tree.reached("src/a.rs", &["Color.Red"], None),
        ["src/b.rs@1"]
    );
    assert_eq!(
        tree.reached("src/a.rs", &["Color.Blue"], None),
        Vec::<String>::new()
    );
}

#[scenario("rust", "A trait imported as underscore still answers by method name")]
#[test]
fn a_trait_imported_as_underscore_still_answers_by_method_name() {
    let tree = crate_of(
        &["a", "b"],
        &[
            (
                "src/a.rs",
                "pub trait Shout {\n    fn shout(&self);\n}\n\nimpl Shout for u8 {\n    fn shout(&self) {}\n}\n",
            ),
            (
                "src/b.rs",
                "use crate::a::Shout as _;\n\npub fn loud<T>(value: T) {\n    value.shout();\n}\n",
            ),
        ],
    );

    let reached = tree.reached("src/a.rs", &["Shout.shout"], None);
    assert!(reached.contains(&"src/b.rs@1".to_owned()), "{reached:?}");
}

#[scenario("rust", "An impl in another file reaches the callers of its type")]
#[test]
fn an_impl_in_another_file_reaches_the_callers_of_its_type() {
    let tree = crate_of(
        &["model", "ext", "app"],
        &[
            ("src/model.rs", "pub struct Foo;\n"),
            (
                "src/ext.rs",
                "use crate::model::Foo;\n\nimpl Foo {\n    pub fn extra(&self) {}\n}\n",
            ),
            (
                "src/app.rs",
                "use crate::model::Foo;\n\npub fn run(foo: &Foo) {\n    foo.extra();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.declared("src/ext.rs"), ["Foo.extra L4-4"]);
    assert_eq!(tree.uses("src/app.rs", "run"), ["Foo", "Foo.extra"]);
    let reached = tree.reached("src/model.rs", &["Foo"], Some(2));
    assert!(reached.contains(&"src/app.rs@1".to_owned()), "{reached:?}");
    assert_eq!(
        tree.reached("src/ext.rs", &["Foo.extra"], None),
        ["src/app.rs@1"]
    );
}

#[scenario("rust", "A type parameter is read as the trait that bounds it")]
#[test]
fn a_type_parameter_is_read_as_the_trait_that_bounds_it() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub trait Tr {\n    fn m(&self);\n}\n\npub fn bound<T: Tr>(x: T) {\n    x.m();\n}\n\npub fn clause<T>(x: &T)\nwhere\n    T: Send + Tr,\n{\n    x.m();\n}\n\npub fn path<T: Tr>(x: T) {\n    T::m(&x);\n}\n\npub fn free<T>(x: T) {\n    x.m();\n}\n\npub fn output<T: Iterator>(x: T::Item)\nwhere\n    T::Item: Tr,\n{\n    x.m();\n}\n\npub struct Held<T: Tr> {\n    inner: T,\n}\n\nimpl<T: Tr> Held<T> {\n    pub fn own(&self, x: T) {\n        x.m();\n        self.inner.m();\n    }\n}\n",
        )],
    );

    assert_eq!(tree.uses("src/a.rs", "bound"), ["Tr", "Tr.m"]);
    assert_eq!(tree.uses("src/a.rs", "clause"), ["Send", "Tr", "Tr.m"]);
    assert_eq!(tree.uses("src/a.rs", "path"), ["Tr", "Tr.m"]);
    assert_eq!(tree.uses("src/a.rs", "free"), [".m"]);
    assert_eq!(
        tree.uses("src/a.rs", "output"),
        [".m", "Iterator", "Iterator.Item", "Tr"]
    );
    assert_eq!(
        tree.uses("src/a.rs", "Held.own"),
        ["Held", "Held.inner", "Tr", "Tr.m"]
    );
}

#[scenario("rust", "An import nothing names is used as the file loads")]
#[test]
fn an_import_nothing_names_is_used_as_the_file_loads() {
    let tree = crate_of(
        &["tr", "named", "nameless", "under", "standard"],
        &[
            ("src/tr.rs", "pub trait Tr {\n    fn m(&self);\n}\n"),
            (
                "src/named.rs",
                "use crate::tr::Tr;\n\npub fn run(x: &dyn Tr) {\n    x.m();\n}\n",
            ),
            (
                "src/nameless.rs",
                "use crate::tr::Tr;\n\npub fn run(x: u8) {\n    x.m();\n}\n",
            ),
            (
                "src/under.rs",
                "use crate::tr::Tr as _;\n\npub fn run(x: u8) {\n    x.m();\n}\n",
            ),
            (
                "src/standard.rs",
                "use std::fmt::Write;\n\npub fn run(x: &mut String) {\n    x.write_str(\"\").ok();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.on_load("src/named.rs"), [] as [&str; 0]);
    assert_eq!(tree.on_load("src/nameless.rs"), ["Tr"]);
    assert_eq!(tree.on_load("src/under.rs"), ["_"]);
    assert_eq!(tree.on_load("src/standard.rs"), [] as [&str; 0]);
    assert_eq!(
        tree.reached("src/tr.rs", &["Tr"], None),
        ["src/named.rs@1", "src/nameless.rs@1", "src/under.rs@1"]
    );
}

#[scenario(
    "rust",
    "A value of a written type reads a member only with a nameless trait in scope"
)]
#[test]
fn a_value_of_a_written_type_reads_a_member_only_with_a_nameless_trait_in_scope() {
    let tree = crate_of(
        &["tr", "model", "other", "imp", "caller", "bystander"],
        &[
            ("src/tr.rs", "pub trait Tr {\n    fn m(&self);\n}\n"),
            ("src/model.rs", "pub struct Foo;\n"),
            (
                "src/other.rs",
                "pub struct Bar;\n\nimpl Bar {\n    pub fn m(&self) {}\n}\n",
            ),
            (
                "src/imp.rs",
                "use crate::model::Foo;\nuse crate::tr::Tr;\n\nimpl Tr for Foo {\n    fn m(&self) {}\n}\n",
            ),
            (
                "src/caller.rs",
                "use crate::model::Foo;\nuse crate::tr::Tr;\n\npub fn run(foo: &Foo) {\n    foo.m();\n}\n",
            ),
            (
                "src/bystander.rs",
                "use crate::model::Foo;\nuse crate::other::Bar;\n\npub fn run(foo: &Foo, _bar: &Bar) {\n    foo.m();\n}\n",
            ),
        ],
    );

    assert_eq!(
        tree.uses("src/caller.rs", "run"),
        [".m", "Foo", "Foo.m"],
        "the trait is in scope for its methods"
    );
    assert_eq!(tree.reads("src/caller.rs"), ["m"]);
    assert_eq!(
        tree.uses("src/bystander.rs", "run"),
        ["Bar", "Foo", "Foo.m"]
    );
    assert_eq!(tree.reads("src/bystander.rs"), [] as [&str; 0]);
    let trait_asked = tree.reached("src/tr.rs", &["Tr.m"], None);
    assert!(
        trait_asked.contains(&"src/caller.rs@1".to_owned()),
        "{trait_asked:?}"
    );
    assert_eq!(
        tree.reached("src/other.rs", &["Bar.m"], None),
        [] as [String; 0]
    );
}

#[scenario("rust", "An implementation is reached through its trait")]
#[test]
fn an_implementation_is_reached_through_its_trait() {
    let tree = crate_of(
        &["tr", "imp", "user"],
        &[
            ("src/tr.rs", "pub trait Tr {\n    fn go(&self);\n}\n"),
            (
                "src/imp.rs",
                "use crate::tr::Tr;\n\npub struct Bash;\n\nimpl Tr for Bash {\n    fn go(&self) {}\n}\n",
            ),
            (
                "src/user.rs",
                "use crate::imp::Bash;\nuse crate::tr::Tr;\n\npub fn run(bash: &Bash) {\n    bash.go();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.heirs("src/imp.rs"), ["Bash Tr"]);
    assert_eq!(tree.overrides("src/imp.rs"), ["Bash Tr"]);
    let reached = tree.reached("src/tr.rs", &["Tr"], Some(1));
    assert!(reached.contains(&"src/imp.rs@1".to_owned()), "{reached:?}");
}

#[scenario("rust", "A field of a class type holds a value of it")]
#[test]
fn a_field_of_a_class_type_holds_a_value_of_it() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct Style;\npub struct Config {\n    pub style: Style,\n    pub maybe: Option<Style>,\n    pub shared: std::sync::Arc<Style>,\n}\n",
        )],
    );

    assert_eq!(
        tree.holds("src/a.rs"),
        [
            "Config.maybe Style",
            "Config.shared Style",
            "Config.style Style"
        ]
    );
}

#[scenario("rust", "A method that returns a class holds a value of it")]
#[test]
fn a_method_that_returns_a_class_holds_a_value_of_it() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct ValueRange;\npub struct Error;\npub struct Arg;\n\nimpl Arg {\n    pub fn by_ref(&self) -> &ValueRange {\n        todo!()\n    }\n\n    pub fn get_num_args(&self) -> Option<ValueRange> {\n        None\n    }\n\n    pub fn checked(&self) -> Result<ValueRange, Error> {\n        todo!()\n    }\n}\n",
        )],
    );

    assert_eq!(
        tree.holds("src/a.rs"),
        [
            "Arg.by_ref ValueRange",
            "Arg.checked ValueRange",
            "Arg.get_num_args ValueRange"
        ]
    );
}

#[scenario("rust", "An associated function without self holds nothing")]
#[test]
fn an_associated_function_without_self_holds_nothing() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct Style;\npub struct Config;\n\nimpl Config {\n    pub fn default_style() -> Style {\n        Style\n    }\n}\n",
        )],
    );

    assert_eq!(tree.holds("src/a.rs"), [] as [String; 0]);
}

#[scenario(
    "rust",
    "A member of its own class or of the standard library holds nothing"
)]
#[test]
fn a_member_of_its_own_class_or_of_the_standard_library_holds_nothing() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct Foo {\n    name: String,\n}\n\nimpl Foo {\n    pub fn with(self) -> Self {\n        self\n    }\n\n    pub fn again(&self) -> Foo {\n        todo!()\n    }\n\n    pub fn len(&self) -> usize {\n        0\n    }\n}\n",
        )],
    );

    assert_eq!(tree.holds("src/a.rs"), [] as [String; 0]);
}

#[scenario("rust", "A member read right on another is read as a pair")]
#[test]
fn a_member_read_right_on_another_is_read_as_a_pair() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub fn run(x: u8) {\n    self.config.style.changes();\n    (&self.name).len();\n}\n",
        )],
    );

    let reads = tree.reads("src/a.rs");
    for pair in ["config.style", "style.changes", "name.len"] {
        assert!(reads.contains(&pair), "{reads:?}");
    }
}

#[scenario("rust", "A pair goes through a call that only hands on the value")]
#[test]
fn a_pair_goes_through_a_call_that_only_hands_on_the_value() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub fn run(arg: u8) -> Option<u8> {\n    arg.range().expect(\"x\").min();\n    arg.other.as_ref().unwrap().max();\n    arg.third()?.top();\n    arg.fourth().await.low();\n    None\n}\n",
        )],
    );

    let reads = tree.reads("src/a.rs");
    for pair in ["range.min", "other.max", "third.top", "fourth.low"] {
        assert!(reads.contains(&pair), "{reads:?}");
    }
}

#[scenario("rust", "A call that changes the value breaks the pair")]
#[test]
fn a_call_that_changes_the_value_breaks_the_pair() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub fn run(arg: u8) {\n    arg.range().map(f).min();\n}\n",
        )],
    );

    let reads = tree.reads("src/a.rs");
    assert!(!reads.contains(&"range.min"), "{reads:?}");
    assert!(reads.contains(&"map.min"), "{reads:?}");
}

#[scenario("rust", "A derive names the trait it derives")]
#[test]
fn a_derive_names_the_trait_it_derives() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "#[derive(Debug, Clone, serde::Serialize)]\npub struct Foo {\n    pub n: u8,\n}\n",
        )],
    );

    let uses = tree.uses("src/a.rs", "Foo");
    assert!(
        uses.contains(&"Serialize") && uses.contains(&"Debug"),
        "{uses:?}"
    );
    assert_eq!(
        tree.heirs("src/a.rs"),
        ["Foo Clone", "Foo Debug", "Foo Serialize"]
    );
}

#[scenario("rust", "The arguments of a macro are read")]
#[test]
fn the_arguments_of_a_macro_are_read() {
    let tree = crate_of(
        &["a", "b"],
        &[
            (
                "src/a.rs",
                "pub fn foo() -> u8 {\n    1\n}\n\npub struct Runner;\n\nimpl Runner {\n    pub fn start(&self) {}\n}\n",
            ),
            (
                "src/b.rs",
                "use crate::a::{foo, Runner};\n\npub fn run<T>(value: T) {\n    println!(\"{}\", foo());\n    assert!(Runner::start(&Runner) == ());\n    dbg!(value.start());\n}\n",
            ),
        ],
    );

    let uses = tree.uses("src/b.rs", "run");
    assert!(
        uses.contains(&"foo") && uses.contains(&"Runner.start") && uses.contains(&".start"),
        "{uses:?}"
    );
}

#[scenario("rust", "A file with syntax errors records no declarations")]
#[test]
fn a_file_with_syntax_errors_records_no_declarations() {
    let tree = crate_of(
        &["a", "b", "c"],
        &[
            ("src/a.rs", "pub fn foo() {}\n"),
            (
                "src/b.rs",
                "use crate::a::foo;\n\npub fn run( {\n    foo();\n}\n",
            ),
            (
                "src/c.rs",
                "use crate::b::run;\n\npub fn main() {\n    run();\n}\n",
            ),
        ],
    );

    assert!(!tree.records("src/b.rs"));
    assert_eq!(
        tree.reached("src/a.rs", &["foo"], Some(2)),
        ["src/b.rs@1", "src/c.rs@2"]
    );
}

#[scenario("rust", "A local rebound to another type reaches both types")]
#[test]
fn a_local_rebound_to_another_type_reaches_both_types() {
    let tree = crate_of(
        &["a", "b"],
        &[
            (
                "src/a.rs",
                "pub struct A;\npub struct B;\n\nimpl A {\n    pub fn run(&self) {}\n}\n\nimpl B {\n    pub fn run(&self) {}\n}\n",
            ),
            (
                "src/b.rs",
                "use crate::a::{A, B};\n\npub fn go() {\n    let x: A = A;\n    x.run();\n    let x: B = B;\n    x.run();\n}\n",
            ),
        ],
    );

    assert_eq!(tree.uses("src/b.rs", "go"), [".run", "A", "B"]);
    assert_eq!(tree.reached("src/a.rs", &["A.run"], None), ["src/b.rs@1"]);
    assert_eq!(tree.reached("src/a.rs", &["B.run"], None), ["src/b.rs@1"]);
}

#[test]
fn a_field_and_a_method_of_one_name_are_one_declaration() {
    let tree = crate_of(
        &["a"],
        &[(
            "src/a.rs",
            "pub struct Job {\n    pub name: String,\n}\n\nimpl Job {\n    pub fn name(&self) -> &str {\n        &self.name\n    }\n\n    pub fn rename(&mut self) {\n        self.name();\n    }\n}\n",
        )],
    );

    assert_eq!(
        tree.declared("src/a.rs"),
        ["Job L1-3", "Job.name L2-8", "Job.rename L10-12"]
    );
    assert_eq!(tree.uses("src/a.rs", "Job.rename"), ["Job", "Job.name"]);
}

#[scenario("rust", "A use keeps the line it is written on")]
#[test]
fn a_use_keeps_the_line_it_is_written_on() {
    let tree = crate_of(
        &["a", "b"],
        &[
            (
                "src/a.rs",
                "pub struct Foo;\n\nimpl Foo {\n    pub fn bar() {}\n}\n\npub fn helper() {}\n",
            ),
            (
                "src/b.rs",
                "use crate::a::{helper, Foo};\n\npub fn run<T>(value: T) {\n    helper();\n    Foo::bar();\n    value\n        .start();\n}\n",
            ),
        ],
    );

    let lines = tree.use_lines("src/b.rs", "run");
    let line_of = |name: &str| {
        lines
            .iter()
            .find(|(used, _)| used == name)
            .map(|(_, line)| *line)
    };
    assert_eq!(line_of("helper"), Some(4), "{lines:?}");
    assert_eq!(line_of("Foo.bar"), Some(5), "{lines:?}");
    // The receiver stands on line 6 and the member it reads on line 7.
    assert_eq!(
        tree.read_lines("src/b.rs"),
        [("start".to_owned(), 7)],
        "{lines:?}"
    );
}
