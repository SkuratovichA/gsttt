//! The text a name stands for in the file that declares it, found before anything else is read, and the names the file imports.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::{
    BindingPattern, ClassElement, Declaration, Expression, ImportDeclaration,
    ImportDeclarationSpecifier, ObjectPropertyKind, Program, Statement, TSEnumMemberName,
    VariableDeclarationKind,
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
    /// The local names an import binds.
    imported: BTreeSet<String>,
}

impl Constants {
    /// The constants `program` declares at its top level, in declaration order, so that a value may use a constant above it, and the names its imports bind.
    pub(crate) fn collect(program: &Program<'_>) -> Self {
        let mut constants = Self::default();
        for statement in &program.body {
            match statement {
                Statement::ImportDeclaration(import) => {
                    for local in import_locals(import) {
                        constants.import(local);
                    }
                }
                Statement::ExportDeclaration(export) => constants.declaration(&export.declaration),
                other => {
                    if let Some(declaration) = other.as_declaration() {
                        constants.declaration(declaration);
                    }
                }
            }
        }
        constants
    }

    /// Notes that the file binds `local` to something another module exports.
    pub(super) fn import(&mut self, local: &str) {
        if !self.imported.contains(local) {
            self.imported.insert(local.to_owned());
        }
    }

    /// The text that `expr` stands for when it is a name or a chain of names that a constant of this file is read under.
    pub(crate) fn value(&self, expr: &Expression<'_>) -> Option<&str> {
        if self.values.is_empty() || !self.roots.contains(base_name(expr)?) {
            return None;
        }
        self.values.get(&*dotted_name(expr)?).map(String::as_str)
    }

    /// The name that `expr` writes when it is a name or a chain of names that starts from a name the file imports: `Env.API_URL` for `Env.API_URL`.
    pub(crate) fn imported_name(&self, expr: &Expression<'_>) -> Option<String> {
        if self.imported.is_empty() || !self.imported.contains(base_name(expr)?) {
            return None;
        }
        dotted_name(expr).map(std::borrow::Cow::into_owned)
    }

    /// The constants whose first segment `exports` name as a local, sorted by name, to be read by the files that import them.
    pub(crate) fn exported(&self, exports: &[Export]) -> Vec<(String, String)> {
        let locals: BTreeSet<&str> = exports
            .iter()
            .filter_map(|export| match &export.target {
                ExportTarget::Local(local) => Some(local.as_str()),
                ExportTarget::From { .. } => None,
            })
            .collect();
        self.values
            .iter()
            .filter(|(name, _)| locals.contains(root_of(name)))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    fn declaration(&mut self, declaration: &Declaration<'_>) {
        match declaration {
            Declaration::VariableDeclaration(variables)
                if variables.kind == VariableDeclarationKind::Const =>
            {
                for declarator in &variables.declarations {
                    if let (BindingPattern::BindingIdentifier(id), Some(init)) =
                        (&declarator.id, &declarator.init)
                    {
                        self.add(&mut id.name.as_str().to_owned(), init);
                    }
                }
            }
            Declaration::TSEnumDeclaration(declared) => {
                for member in &declared.body.members {
                    let name = match &member.id {
                        TSEnumMemberName::Identifier(name) => Some(name.name.as_str()),
                        TSEnumMemberName::String(name) => name.value.as_str(),
                        TSEnumMemberName::ComputedString(_)
                        | TSEnumMemberName::ComputedTemplateString(_) => None,
                    };
                    if let (Some(name), Some(init)) = (name, &member.initializer) {
                        let mut path = format!("{}.{name}", declared.id.name);
                        self.add(&mut path, init);
                    }
                }
            }
            Declaration::ClassDeclaration(class) => {
                let Some(id) = &class.id else {
                    return;
                };
                for element in &class.body.body {
                    if let ClassElement::PropertyDefinition(field) = element
                        && field.r#static
                        && let (Some(name), Some(init)) = (field_name(field), &field.value)
                    {
                        let mut path = format!("{}.{name}", id.name);
                        self.add(&mut path, init);
                    }
                }
            }
            _ => {}
        }
    }

    /// Records `init` under `path`, or, for an object literal, each of its properties under `path` and the key.
    fn add(&mut self, path: &mut String, init: &Expression<'_>) {
        let Expression::ObjectExpression(object) = init.get_inner_expression() else {
            if let Some(value) = self.literal(init) {
                self.insert(path, value);
            }
            return;
        };
        for property in &object.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                continue;
            };
            // A key with a dot would read as two segments.
            let Some(key) =
                property_str(&property.key).filter(|key| !property.computed && !key.contains('.'))
            else {
                continue;
            };
            let length = path.len();
            path.push('.');
            path.push_str(key);
            self.add(path, &property.value);
            path.truncate(length);
        }
    }

    fn insert(&mut self, path: &str, value: String) {
        let root = root_of(path);
        if !self.roots.contains(root) {
            self.roots.insert(root.to_owned());
        }
        self.values.insert(path.to_owned(), value);
    }

    /// The text of a string, a template or a concatenation whose every part is a constant declared above; `None` when any part is anything else.
    pub(crate) fn literal(&self, expr: &Expression<'_>) -> Option<String> {
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

/// The first segment of a dotted name.
fn root_of(name: &str) -> &str {
    name.split('.').next().unwrap_or(name)
}
