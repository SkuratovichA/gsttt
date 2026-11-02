//! The text a name stands for in the file that declares it, found before anything else is read, and the names the file imports.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclaration, ImportDeclarationSpecifier, ObjectPropertyKind, Program, Statement,
    TSEnumMemberName, TSModuleReference, VariableDeclarationKind,
};

use super::roots::rebound;
use super::symbols::base_name;
use super::{load_of, peel};
use crate::extra::{Export, ExportTarget};
use crate::parts::{Mode, Part};
use crate::shape::{dotted_name, field_name, literal_text, part_of, property_str};

/// The top-level constants of a file by the name they are read under, and the names the file imports.
#[derive(Debug, Default)]
pub(crate) struct Constants {
    /// `BASE`, `Env.API_URL`, `Routes.LOGIN` and `ROUTES.users.byId` to the text they stand for, which is put together from names that other files declare when the constant is built from some.
    values: BTreeMap<String, Part>,
    /// The first segment of each name in `values`, so that a name that starts with anything else is known to be no constant before a string is built for it.
    roots: BTreeSet<String>,
    /// The local names an import binds, each with the specifier of the module it takes the name from.
    imported: BTreeMap<String, String>,
    /// The imported names that the file binds again, as a parameter, a variable, a function or a class. The file is not read by scope, so such a name is no import for the value a rule reads anywhere in it.
    rebound: BTreeSet<String>,
    /// The enum whose members are being read, while they are.
    scope: Option<EnumScope>,
}

/// The enum being read: a bare name in the initializer of a member is the member of that name, when the enum has declared one above, as TypeScript scopes it.
#[derive(Debug)]
struct EnumScope {
    name: String,
    members: BTreeSet<String>,
}

impl Constants {
    /// The constants `program` declares at its top level, in declaration order, so that a value may use a constant above it, and the names its imports bind.
    pub(crate) fn collect(program: &Program<'_>) -> Self {
        let mut constants = Self::default();
        // Every import is known before a constant is read: an `import` is hoisted, so a constant may name one that is written below it, and a name that `require` binds is the module's wherever it is read.
        for statement in &program.body {
            constants.imports(statement);
        }
        if !constants.imported.is_empty() {
            let names = constants.imported.keys().map(String::as_str);
            constants.rebound = rebound(program, names);
        }
        // One buffer holds the name of every declaration in turn, so that the names of the many declarations that hold no text cost nothing.
        let mut path = String::new();
        for statement in &program.body {
            match statement {
                Statement::ExportDeclaration(export) => {
                    constants.declaration(&export.declaration, &mut path);
                }
                Statement::ExportDefaultDeclaration(export) => {
                    if let ExportDefaultDeclarationKind::ClassDeclaration(class) =
                        &export.declaration
                    {
                        constants.statics(class, &mut path);
                    }
                }
                other => {
                    if let Some(declaration) = other.as_declaration() {
                        constants.declaration(declaration, &mut path);
                    }
                }
            }
        }
        constants
    }

    /// Notes the names that `statement` binds to a module: those of an `import`, of `import x = require('y')`, and of `const { a } = require('y')` and its dynamic kin, as the walk of the file binds them.
    fn imports(&mut self, statement: &Statement<'_>) {
        let declaration = match statement {
            Statement::ImportDeclaration(import) => {
                if let Some(spec) = import.source.value.as_str() {
                    for local in import_locals(import) {
                        self.import(local, spec);
                    }
                }
                return;
            }
            Statement::ExportDeclaration(export) => &export.declaration,
            other => match other.as_declaration() {
                Some(declaration) => declaration,
                None => return,
            },
        };
        match declaration {
            Declaration::TSImportEqualsDeclaration(equals) => {
                if let TSModuleReference::ExternalModuleReference(reference) =
                    &equals.module_reference
                    && let Some(spec) = reference.expression.value.as_str()
                {
                    self.import(equals.id.name.as_str(), spec);
                }
            }
            Declaration::VariableDeclaration(variables) => {
                for declarator in &variables.declarations {
                    if let Some(init) = &declarator.init
                        && let Some(load) = load_of(peel(init))
                        && let Some(spec) = load.argument.and_then(literal_text)
                    {
                        self.loaded(&declarator.id, &spec);
                    }
                }
            }
            _ => {}
        }
    }

    /// Notes the names that `pattern` takes from the module `spec` that a load hands back: `x` of `const x = require('y')`, and each name of `{ a, b: c }` that is written as a name and bound to one.
    fn loaded(&mut self, pattern: &BindingPattern<'_>, spec: &str) {
        match pattern {
            BindingPattern::BindingIdentifier(local) => self.import(local.name.as_str(), spec),
            BindingPattern::ObjectPattern(object) => {
                for property in &object.properties {
                    if !property.computed
                        && property_str(&property.key).is_some()
                        && let BindingPattern::BindingIdentifier(local) = &property.value
                    {
                        self.import(local.name.as_str(), spec);
                    }
                }
            }
            _ => {}
        }
    }

    /// Notes that the file binds `local` to something the module `spec` exports.
    pub(super) fn import(&mut self, local: &str, spec: &str) {
        if !self.imported.contains_key(local) {
            self.imported.insert(local.to_owned(), spec.to_owned());
        }
    }

    /// The specifier of the module that `local` is imported from, when the file imports the name.
    pub(super) fn spec_of(&self, local: &str) -> Option<&str> {
        self.imported.get(local).map(String::as_str)
    }

    /// What `expr` stands for when it is a name or a chain of names that a constant of this file is read under, or that starts from a name the file imports: `Env.API_URL` for `Env.API_URL`, which the link phase reads. A constant is lent, not copied, so a use of it costs nothing until a part of it is kept.
    pub(crate) fn part(&self, expr: &Expression<'_>) -> Option<Cow<'_, Part>> {
        if let Some(scope) = &self.scope
            && let Expression::Identifier(identifier) = expr.get_inner_expression()
            && scope.members.contains(identifier.name.as_str())
        {
            // A sibling hides a constant of the same name, even when the sibling is no text.
            return self
                .values
                .get(&format!("{}.{}", scope.name, identifier.name))
                .map(Cow::Borrowed);
        }
        self.constant(expr).map(Cow::Borrowed).or_else(|| {
            self.imported_name(expr)
                .map(|name| Cow::Owned(Part::Name(name)))
        })
    }

    /// The constant of this file that `expr` is the name of.
    fn constant(&self, expr: &Expression<'_>) -> Option<&Part> {
        if self.values.is_empty() || !self.roots.contains(base_name(expr)?) {
            return None;
        }
        self.values.get(&*dotted_name(expr)?)
    }

    /// The name that `expr` writes when it is a name or a chain of names that starts from a name the file imports, and binds nowhere else: `Env.API_URL` for `Env.API_URL`.
    fn imported_name(&self, expr: &Expression<'_>) -> Option<String> {
        if self.imported.is_empty() {
            return None;
        }
        let base = base_name(expr)?;
        if !self.imported.contains_key(base) || self.rebound.contains(base) {
            return None;
        }
        dotted_name(expr).map(Cow::into_owned)
    }

    /// The constants whose first segment `exports` name as a local, sorted by name, to be read by the files that import them.
    pub(crate) fn exported(&self, exports: &[Export]) -> Vec<(String, Part)> {
        let locals = exported_locals(exports);
        self.values
            .iter()
            .filter(|(name, _)| locals.contains(root_of(name)))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    fn declaration(&mut self, declaration: &Declaration<'_>, path: &mut String) {
        match declaration {
            Declaration::VariableDeclaration(variables)
                if variables.kind == VariableDeclarationKind::Const =>
            {
                for declarator in &variables.declarations {
                    if let (BindingPattern::BindingIdentifier(id), Some(init)) =
                        (&declarator.id, &declarator.init)
                        && may_hold_text(init)
                    {
                        rewrite(path, &[id.name.as_str()]);
                        self.add(path, init);
                    }
                }
            }
            Declaration::TSEnumDeclaration(declared) => {
                self.scope = Some(EnumScope {
                    name: declared.id.name.as_str().to_owned(),
                    members: BTreeSet::new(),
                });
                for member in &declared.body.members {
                    let name = match &member.id {
                        TSEnumMemberName::Identifier(name) => Some(name.name.as_str()),
                        TSEnumMemberName::String(name) => name.value.as_str(),
                        TSEnumMemberName::ComputedString(_)
                        | TSEnumMemberName::ComputedTemplateString(_) => None,
                    };
                    let Some(name) = name else {
                        continue;
                    };
                    if let Some(init) = &member.initializer
                        && may_hold_text(init)
                    {
                        rewrite(path, &[declared.id.name.as_str(), name]);
                        self.add(path, init);
                    }
                    // The member is a sibling for the initializers below it, whether or not it holds text.
                    if let Some(scope) = &mut self.scope {
                        scope.members.insert(name.to_owned());
                    }
                }
                self.scope = None;
            }
            Declaration::ClassDeclaration(class) => self.statics(class, path),
            _ => {}
        }
    }

    /// The static fields of `class` that hold text, under the name of the class.
    fn statics(&mut self, class: &Class<'_>, path: &mut String) {
        let Some(id) = &class.id else {
            return;
        };
        for element in &class.body.body {
            if let ClassElement::PropertyDefinition(field) = element
                && field.r#static
                && let (Some(name), Some(init)) = (field_name(field), &field.value)
                && may_hold_text(init)
            {
                rewrite(path, &[id.name.as_str(), name]);
                self.add(path, init);
            }
        }
    }

    /// Records `init` under `path`, or, for an object literal, each of its properties under `path` and the key. A spread may give any key another value, so it takes back the keys read above it, and the keys below it stay.
    fn add(&mut self, path: &mut String, init: &Expression<'_>) {
        let Expression::ObjectExpression(object) = init.get_inner_expression() else {
            if let Some(value) = part_of(init, self, Mode::Declare) {
                self.insert(path, value);
            }
            return;
        };
        for property in &object.properties {
            let property = match property {
                ObjectPropertyKind::ObjectProperty(property) => property,
                ObjectPropertyKind::SpreadProperty(_) => {
                    self.forget_below(path);
                    continue;
                }
            };
            // A key with a dot would read as two segments.
            let Some(key) =
                property_str(&property.key).filter(|key| !property.computed && !key.contains('.'))
            else {
                continue;
            };
            if !may_hold_text(&property.value) {
                continue;
            }
            let length = path.len();
            path.push('.');
            path.push_str(key);
            self.add(path, &property.value);
            path.truncate(length);
        }
    }

    /// Forgets every name below `path`.
    fn forget_below(&mut self, path: &str) {
        self.values.retain(|name, _| {
            !name
                .strip_prefix(path)
                .is_some_and(|rest| rest.starts_with('.'))
        });
    }

    fn insert(&mut self, path: &str, value: Part) {
        let root = root_of(path);
        if !self.roots.contains(root) {
            self.roots.insert(root.to_owned());
        }
        self.values.insert(path.to_owned(), value);
    }
}

/// The local names that `import` binds.
pub(super) fn import_locals<'a>(import: &ImportDeclaration<'a>) -> impl Iterator<Item = &'a str> {
    import
        .specifiers
        .iter()
        .flatten()
        .map(|specifier| match specifier {
            ImportDeclarationSpecifier::ImportSpecifier(item) => item.local.name.as_str(),
            ImportDeclarationSpecifier::ImportDefaultSpecifier(item) => item.local.name.as_str(),
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(item) => item.local.name.as_str(),
        })
}

/// Whether `init` can stand for text or for an object that holds some. Any other initializer (a call, a function, a number) is no constant, and nothing is built for its name.
fn may_hold_text(init: &Expression<'_>) -> bool {
    matches!(
        init.get_inner_expression(),
        Expression::StringLiteral(_)
            | Expression::TemplateLiteral(_)
            | Expression::BinaryExpression(_)
            | Expression::ObjectExpression(_)
            | Expression::Identifier(_)
            | Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::ChainExpression(_)
    )
}

/// Writes the dotted name of `segments` into `path`, in place of what it held.
fn rewrite(path: &mut String, segments: &[&str]) {
    path.clear();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            path.push('.');
        }
        path.push_str(segment);
    }
}

/// The names a file exports as its own declarations, the locals that `exports` name: the ones other files can read a constant or a maker from.
pub(super) fn exported_locals(exports: &[Export]) -> BTreeSet<&str> {
    exports
        .iter()
        .filter_map(|export| match &export.target {
            ExportTarget::Local(local) => Some(local.as_str()),
            ExportTarget::From { .. } => None,
        })
        .collect()
}

/// The first segment of a dotted name.
fn root_of(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use proptest::prelude::*;

    use std::collections::BTreeMap;

    use gsttt_core::fact::FileTags;
    use gsttt_core::text::LineIndex;

    use super::Constants;
    use crate::analyze::analyze;
    use crate::extra::{Export, ExportTarget};
    use crate::parts::{Mode, Part};
    use crate::shape::part_of;

    fn constants_of(code: &str) -> Constants {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        Constants::collect(&parsed.program)
    }

    /// The constants of `code` as `name=value` lines in name order, with a name that another file declares written `<NAME>`.
    fn read(code: &str) -> Vec<String> {
        constants_of(code)
            .values
            .iter()
            .map(|(name, value)| {
                let text = value.read(&|name| Some(format!("<{name}>")));
                format!("{name}={}", text.unwrap_or_default())
            })
            .collect()
    }

    #[test]
    fn a_bare_name_in_an_enum_is_the_member_of_that_name_first() {
        assert_eq!(
            read("const A = '/top'; enum E { A = '/a', B = A, C = `${A}/c`, D = E.A }"),
            ["A=/top", "E.A=/a", "E.B=/a", "E.C=/a/c", "E.D=/a"]
        );
    }

    #[test]
    fn a_member_that_is_no_text_still_hides_the_constant_of_its_name() {
        assert_eq!(
            read("const A = '/top'; enum E { A = 1, B = A }"),
            ["A=/top"]
        );
    }

    #[test]
    fn a_name_that_is_no_member_is_read_from_the_file() {
        assert_eq!(
            read("const T = '/t'; enum E { X = T, Y = X }"),
            ["E.X=/t", "E.Y=/t", "T=/t"]
        );
        // A member below does not hide the constant above it.
        assert_eq!(
            read("const Z = '/top'; enum E { A = Z, Z = '/z' }"),
            ["E.A=/top", "E.Z=/z", "Z=/top"]
        );
    }

    #[test]
    fn the_scope_of_an_enum_ends_with_it() {
        assert_eq!(read("enum E { A = '/a' } const B = A;"), ["E.A=/a"]);
    }

    #[test]
    fn a_spread_takes_back_the_keys_above_it_and_leaves_the_keys_below() {
        assert_eq!(read("const R = { a: '/a', ...OTHER, b: '/b' }"), ["R.b=/b"]);
        assert_eq!(
            read("const R = { a: '/a', nested: { x: '/x', ...O, y: '/y' }, b: '/b' }"),
            ["R.a=/a", "R.b=/b", "R.nested.y=/y"]
        );
        assert_eq!(
            read("const R = { a: '/a', ...O }; const S = { c: '/c' }"),
            ["S.c=/c"]
        );
    }

    #[test]
    fn a_name_that_only_starts_like_the_object_is_not_taken_back() {
        assert_eq!(
            read("const R = { a: '/a' }; const R2 = { b: '/b', ...O }"),
            ["R.a=/a"]
        );
    }

    #[test]
    fn the_statics_of_a_default_exported_class_are_read_and_exported() {
        let code = "export default class Routes { static LOGIN = '/login'; other = '/o'; }";
        assert_eq!(read(code), ["Routes.LOGIN=/login"]);
        let exports = [Export {
            name: "default".to_owned(),
            target: ExportTarget::Local("Routes".to_owned()),
        }];
        assert_eq!(
            constants_of(code).exported(&exports),
            [("Routes.LOGIN".to_owned(), Part::Text("/login".to_owned()))]
        );
        assert_eq!(
            read("export default class { static LOGIN = '/login'; }"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_declaration_that_holds_no_text_leaves_no_name() {
        assert_eq!(
            read(
                "const a = 1; const b = () => '/b'; const c = f('/c'); const d = ['/d']; const e = new URL('/e', base);"
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_constant_built_from_a_name_another_file_declares_is_put_together_at_the_link() {
        let code = "import { BASE, Env } from './config'; const T = `${BASE}/t`; const P = BASE + '/p'; const E = `${Env.API}/e`; const A = BASE;";
        assert_eq!(
            read(code),
            ["A=<BASE>", "E=<Env.API>/e", "P=<BASE>/p", "T=<BASE>/t"]
        );
    }

    #[test]
    fn a_constant_built_from_such_a_constant_is_put_together_too() {
        let code = "import { BASE } from './config'; const T = `${BASE}/t`; const U = `${T}/u`; const V = T + '/v';";
        assert_eq!(read(code), ["T=<BASE>/t", "U=<BASE>/t/u", "V=<BASE>/t/v"]);
    }

    #[test]
    fn a_constant_with_a_part_that_is_no_text_is_no_constant_whatever_else_it_names() {
        assert_eq!(
            read(
                "import { BASE } from './config'; const T = `${BASE}/${id()}`; const U = BASE + f();"
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_name_that_a_sibling_of_the_enum_hides_is_no_import() {
        assert_eq!(
            read("import { A } from './a'; enum E { A = 1, B = A }"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_constant_may_name_an_import_written_below_it() {
        assert_eq!(
            read("const U = `${BASE}/u`; const V = BASE + '/v'; import { BASE } from './config';"),
            ["U=<BASE>/u", "V=<BASE>/v"]
        );
        assert_eq!(
            read("const U = `${ns.BASE}/u`; import * as ns from './config';"),
            ["U=<ns.BASE>/u"]
        );
    }

    #[test]
    fn a_constant_may_name_what_a_require_binds() {
        assert_eq!(
            read("const U = `${BASE}/u`; const { BASE } = require('./config');"),
            ["U=<BASE>/u"]
        );
        assert_eq!(
            read("const { BASE } = require('./config'); const U = `${BASE}/u`;"),
            ["U=<BASE>/u"]
        );
        assert_eq!(
            read("import config = require('./config'); const U = `${config.BASE}/u`;"),
            ["U=<config.BASE>/u"]
        );
    }

    /// The local names `code` binds, with the module each takes its name from, as the walk of the file binds them.
    fn walk_binds(code: &str) -> BTreeMap<String, String> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        analyze(
            &parsed.program,
            code,
            &LineIndex::new(code),
            &[],
            FileTags::NONE,
        )
        .extra
        .bindings
        .into_iter()
        .map(|binding| (binding.local, binding.spec))
        .collect()
    }

    /// `Constants::collect` binds the names that the walk of the file does, by its own reading of the same forms, so the order of the two never changes what a name stands for.
    #[test]
    fn the_names_collected_before_the_walk_are_the_names_the_walk_binds() {
        for code in [
            "import a, { b, c as d } from 'x'; import * as ns from 'y'; import type { T } from 'z';",
            "import e = require('e'); export import f = require('f');",
            "const g = require('g'); let h = require('h'); var i = (require('i') as T);",
            "const { j, k: l, 'm': n, ...rest } = require('j');",
            "const { [key]: o, p = 1, q: [r] } = require('o');",
            "const s = await import('s'); const t = jest.requireActual('t'); const u = import('u');",
            "export const v = require('v'); export const { w } = require('w');",
            "const x = require(name); const [y] = require('y'); const z = require(`z`);",
            "import { A } from './a'; const B = require('./b'); const { C } = require('./c');",
        ] {
            assert_eq!(constants_of(code).imported, walk_binds(code), "{code}");
        }
    }

    /// What the expression `source` reads as after the declarations of `prelude`, as the initializer of a constant and as a use.
    fn both(prelude: &str, source: &str) -> (Option<Part>, Option<Part>) {
        let allocator = Allocator::default();
        let code = format!("{prelude}\n({source});");
        let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
        let constants = Constants::collect(&parsed.program);
        let Some(oxc_ast::ast::Statement::ExpressionStatement(statement)) =
            parsed.program.body.last()
        else {
            panic!("no expression in {code}");
        };
        (
            part_of(&statement.expression, &constants, Mode::Declare),
            part_of(&statement.expression, &constants, Mode::Use),
        )
    }

    #[test]
    fn a_constant_that_holds_a_name_is_read_as_a_whole_or_not_at_all_by_a_declaration() {
        let prelude = "import { IMP } from './imp'; const C = `${IMP}/c`;";
        let (declared, used) = both(prelude, "`${C}/x`");
        let nothing = |_: &str| None;
        assert_eq!(declared.and_then(|part| part.read(&nothing)), None);
        assert_eq!(
            used.and_then(|part| part.read(&nothing)),
            Some("${}/x".to_owned())
        );
    }

    #[test]
    fn a_name_that_does_not_read_leaves_a_declaration_no_text_and_a_use_a_computed_part() {
        let prelude = "import { IMP } from './imp';";
        let nothing = |_: &str| None;
        let (declared, used) = both(prelude, "`/a/${IMP}`");
        assert_eq!(declared.and_then(|part| part.read(&nothing)), None);
        assert_eq!(
            used.and_then(|part| part.read(&nothing)),
            Some("/a/${}".to_owned())
        );
        let (declared, used) = both(prelude, "'/a' + UNKNOWN");
        assert_eq!(declared, None);
        assert_eq!(used, Some(Part::Text("/a${}".to_owned())));
    }

    #[test]
    fn a_template_with_no_name_is_one_text() {
        assert_eq!(
            both("const A = '/a';", "`/x/${A}/${f()}/y`").1,
            Some(Part::Text("/x//a/${}/y".to_owned()))
        );
    }

    /// The expressions the generator below writes, over the constants `A`, `B`, `C` and `O`, a name another file declares and a name no constant stands for.
    fn expression() -> impl Strategy<Value = String> {
        let leaf = prop_oneof![
            "[a-z/]{0,4}".prop_map(|text| format!("'{text}'")),
            "[a-z/]{0,4}".prop_map(|text| format!("`{text}`")),
            Just("A".to_owned()),
            Just("B".to_owned()),
            Just("C".to_owned()),
            Just("O.p".to_owned()),
            Just("O.q.r".to_owned()),
            Just("O.q".to_owned()),
            Just("UNKNOWN".to_owned()),
            Just("IMP".to_owned()),
            Just("1".to_owned()),
        ];
        leaf.prop_recursive(3, 12, 2, |inner| {
            prop_oneof![
                (inner.clone(), inner.clone())
                    .prop_map(|(left, right)| format!("{left} + {right}")),
                (inner.clone(), "[a-z/]{0,3}", "[a-z/]{0,3}")
                    .prop_map(|(part, before, after)| format!("`{before}${{{part}}}{after}`")),
                inner.prop_map(|inner| format!("({inner} as const)")),
            ]
        })
    }

    proptest! {
        /// What a declaration reads as a constant is what a use of the expression reads: the two never disagree on text, whether the link reads the name another file declares or reads nothing, and `C` is a constant that holds such a name.
        #[test]
        fn wherever_a_declaration_reads_text_a_use_reads_the_same(source in expression()) {
            let prelude = "import { IMP } from './imp'; const A = '/a'; const B = `${A}/b`; const C = `${IMP}/c`; const O = { p: '/p', q: { r: '/r' } };";
            let (declared, used) = both(prelude, &source);
            for resolved in [Some("IMP"), None] {
                let resolve = |name: &str| (Some(name) == resolved).then(|| "/imp".to_owned());
                if let Some(text) = declared.as_ref().and_then(|part| part.read(&resolve)) {
                    prop_assert_eq!(
                        used.as_ref().and_then(|part| part.read(&resolve)),
                        Some(text),
                        "{} with {:?} read",
                        source,
                        resolved
                    );
                }
            }
        }
    }
}
