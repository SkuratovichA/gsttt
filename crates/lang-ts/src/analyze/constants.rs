//! The text a name stands for in the file that declares it, found before anything else is read, and the names the file imports.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclaration, ImportDeclarationSpecifier, ObjectPropertyKind, Program, Statement,
    TSEnumMemberName, VariableDeclarationKind,
};
use oxc_syntax::operator::BinaryOperator;

use super::symbols::base_name;
use crate::extra::{Export, ExportTarget};
use crate::shape::{dotted_name, field_name, property_str, template_with};

/// The top-level constants of a file by the name they are read under, and the names the file imports.
#[derive(Debug, Default)]
pub(crate) struct Constants {
    /// `BASE`, `Env.API_URL`, `Routes.LOGIN` and `ROUTES.users.byId` to the text they stand for.
    values: BTreeMap<String, String>,
    /// The first segment of each name in `values`, so that a name that starts with anything else is known to be no constant before a string is built for it.
    roots: BTreeSet<String>,
    /// The local names an import binds, each with the specifier of the module it takes the name from.
    imported: BTreeMap<String, String>,
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
        // One buffer holds the name of every declaration in turn, so that the names of the many declarations that hold no text cost nothing.
        let mut path = String::new();
        for statement in &program.body {
            match statement {
                Statement::ImportDeclaration(import) => {
                    let Some(spec) = import.source.value.as_str() else {
                        continue;
                    };
                    for local in import_locals(import) {
                        constants.import(local, spec);
                    }
                }
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

    /// The text that `expr` stands for when it is a name or a chain of names that a constant of this file is read under.
    pub(crate) fn value(&self, expr: &Expression<'_>) -> Option<&str> {
        if let Some(scope) = &self.scope
            && let Expression::Identifier(identifier) = expr.get_inner_expression()
            && scope.members.contains(identifier.name.as_str())
        {
            // A sibling hides a constant of the same name, even when the sibling is no text.
            return self
                .values
                .get(&format!("{}.{}", scope.name, identifier.name))
                .map(String::as_str);
        }
        if self.values.is_empty() || !self.roots.contains(base_name(expr)?) {
            return None;
        }
        self.values.get(&*dotted_name(expr)?).map(String::as_str)
    }

    /// The name that `expr` writes when it is a name or a chain of names that starts from a name the file imports: `Env.API_URL` for `Env.API_URL`.
    pub(crate) fn imported_name(&self, expr: &Expression<'_>) -> Option<String> {
        if self.imported.is_empty() || !self.imported.contains_key(base_name(expr)?) {
            return None;
        }
        dotted_name(expr).map(std::borrow::Cow::into_owned)
    }

    /// The constants whose first segment `exports` name as a local, sorted by name, to be read by the files that import them.
    pub(crate) fn exported(&self, exports: &[Export]) -> Vec<(String, String)> {
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
            if let Some(value) = self.literal(init) {
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

    fn insert(&mut self, path: &str, value: String) {
        let root = root_of(path);
        if !self.roots.contains(root) {
            self.roots.insert(root.to_owned());
        }
        self.values.insert(path.to_owned(), value);
    }

    /// The text of a string, a template or a concatenation whose every part is a constant declared above; `None` when any part is anything else. What a use of the name reads is [`text_with`](crate::shape::text_with), which writes `${}` for a part it does not know; a declaration is a constant only when nothing is unknown, and wherever this gives text so does that, the same.
    fn literal(&self, expr: &Expression<'_>) -> Option<String> {
        match expr.get_inner_expression() {
            Expression::StringLiteral(text) => text.value.as_str().map(str::to_owned),
            Expression::TemplateLiteral(template) => {
                let mut complete = true;
                let text = template_with(template, |part, text| match self.value(part) {
                    Some(value) => text.push_str(value),
                    None => complete = false,
                });
                complete.then_some(text)
            }
            Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
                let mut text = self.literal(&binary.left)?;
                text.push_str(&self.literal(&binary.right)?);
                Some(text)
            }
            other => self.value(other).map(str::to_owned),
        }
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

    use super::Constants;
    use crate::extra::{Export, ExportTarget};
    use crate::shape::text_with;

    fn constants_of(code: &str) -> Constants {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, code, SourceType::ts()).parse();
        Constants::collect(&parsed.program)
    }

    /// The constants of `code` as `name=value` lines in name order.
    fn read(code: &str) -> Vec<String> {
        constants_of(code)
            .values
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
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
            [("Routes.LOGIN".to_owned(), "/login".to_owned())]
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

    /// The expressions the generator below writes, over the constants `A`, `B` and `O` and a name no constant stands for.
    fn expression() -> impl Strategy<Value = String> {
        let leaf = prop_oneof![
            "[a-z/]{0,4}".prop_map(|text| format!("'{text}'")),
            "[a-z/]{0,4}".prop_map(|text| format!("`{text}`")),
            Just("A".to_owned()),
            Just("B".to_owned()),
            Just("O.p".to_owned()),
            Just("O.q.r".to_owned()),
            Just("O.q".to_owned()),
            Just("UNKNOWN".to_owned()),
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
        /// What a declaration reads as a constant is what a use of the expression reads: the two evaluators never disagree on text.
        #[test]
        fn wherever_a_declaration_reads_text_a_use_reads_the_same(source in expression()) {
            let prelude = "const A = '/a'; const B = `${A}/b`; const O = { p: '/p', q: { r: '/r' } };";
            let allocator = Allocator::default();
            let code = format!("{prelude}\n({source});");
            let parsed = Parser::new(&allocator, &code, SourceType::ts()).parse();
            let constants = Constants::collect(&parsed.program);
            let Some(oxc_ast::ast::Statement::ExpressionStatement(statement)) = parsed.program.body.last() else {
                panic!("no expression in {code}");
            };
            if let Some(declared) = constants.literal(&statement.expression) {
                prop_assert_eq!(text_with(&statement.expression, &constants), Some(declared), "{}", source);
            }
        }
    }
}
