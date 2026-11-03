//! Which locals of a function are given values of different types: a read of one of them may land on either, so the code that reads it cannot be said to read one class.

use std::collections::{HashMap, HashSet};

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ExceptHandler, Expr, ExprContext, Pattern, Stmt, StmtFor, StmtFunctionDef, StmtWith,
};

use super::table::{Table, Ty, captured};

/// The locals of `function`, parameters included, that are bound more than once to values that do not all have the same type known to the file. `parameters` holds the types of the annotated ones; a parameter without one starts out as a value of no known type.
///
/// The recorder types a local from the assignment before the read, which is right in a straight line and wrong after a branch or in a loop, where the read may follow either assignment. A local in this set is typed nowhere in the function.
pub(super) fn mixed(
    table: &Table,
    function: &StmtFunctionDef,
    parameters: &HashMap<String, Ty>,
) -> HashSet<String> {
    let mut scan = Scan {
        table,
        first: HashMap::new(),
        types: parameters.clone(),
        mixed: HashSet::new(),
        comprehensions: 0,
    };
    for parameter in &function.parameters {
        let name = parameter.name().as_str();
        scan.first
            .insert(name.to_owned(), parameters.get(name).cloned());
    }
    scan.visit_body(&function.body);
    scan.mixed
}

struct Scan<'t> {
    table: &'t Table,
    /// The type of the first binding of each local, `None` for a value the file cannot tell.
    first: HashMap<String, Option<Ty>>,
    /// What the locals are known to hold at this point of a reading in source order, which is what a value that is a local is typed with.
    types: HashMap<String, Ty>,
    mixed: HashSet<String>,
    /// How many comprehensions deep the walk is: the names they loop over are theirs.
    comprehensions: usize,
}

impl Scan<'_> {
    /// Notes that `name` is bound to a value of type `ty`.
    fn bind(&mut self, name: &str, ty: Option<Ty>) {
        match self.first.get(name) {
            None => {
                self.first.insert(name.to_owned(), ty.clone());
            }
            Some(first) if *first == ty => {}
            Some(_) => {
                self.mixed.insert(name.to_owned());
            }
        }
        match ty {
            Some(ty) if !self.mixed.contains(name) => {
                self.types.insert(name.to_owned(), ty);
            }
            _ => {
                self.types.remove(name);
            }
        }
    }

    /// An assignment of a value of type `ty` to `target`. A name takes the type; a name inside a tuple takes a part of the value, and an attribute or a subscript binds none.
    fn target(&mut self, target: &Expr, ty: Option<Ty>) {
        match target {
            Expr::Name(name) => self.bind(name.id.as_str(), ty),
            other => self.visit_expr(other),
        }
    }
}

impl<'a> Scan<'_> {
    /// A `with` binds its name to what [`Table::with_type`] says, as the recorder reads it.
    fn with(&mut self, with: &'a StmtWith) {
        for item in &with.items {
            self.visit_expr(&item.context_expr);
            if let Some(target) = &item.optional_vars {
                let ty = if with.is_async {
                    None
                } else {
                    self.table.with_type(&item.context_expr, &self.types)
                };
                self.target(target, ty);
            }
        }
        self.visit_body(&with.body);
    }

    /// A `for` binds its variable to what [`Table::iteration_type`] says, as the recorder reads it.
    fn looped(&mut self, looped: &'a StmtFor) {
        self.visit_expr(&looped.iter);
        let ty = if looped.is_async {
            None
        } else {
            self.table.iteration_type(&looped.iter)
        };
        self.target(&looped.target, ty);
        self.visit_body(&looped.body);
        self.visit_body(&looped.orelse);
    }
}

impl<'a> Visitor<'a> for Scan<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // A function or a class has a scope of its own, and binds its name here.
            Stmt::FunctionDef(function) => self.bind(function.name.as_str(), None),
            Stmt::ClassDef(class) => self.bind(class.name.as_str(), None),
            Stmt::Assign(assign) => {
                self.visit_expr(&assign.value);
                let ty = self.table.value_type(&assign.value, &self.types);
                for target in &assign.targets {
                    self.target(target, ty.clone());
                }
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    self.visit_expr(value);
                }
                let ty = self.table.annotation_type(&assign.annotation);
                self.target(&assign.target, ty);
            }
            Stmt::With(with) => self.with(with),
            Stmt::For(looped) => self.looped(looped),
            Stmt::Import(_) | Stmt::ImportFrom(_) => {}
            other => visitor::walk_stmt(self, other),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name)
                if matches!(name.ctx, ExprContext::Store | ExprContext::Del)
                    && self.comprehensions == 0 =>
            {
                self.bind(name.id.as_str(), None);
            }
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                if let Expr::Name(target) = &*named.target {
                    let ty = self.table.value_type(&named.value, &self.types);
                    self.bind(target.id.as_str(), ty);
                }
            }
            // A lambda is a scope of its own, and so is a comprehension, except for the walrus in it.
            Expr::Lambda(_) => {}
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {
                self.comprehensions += 1;
                visitor::walk_expr(self, expr);
                self.comprehensions -= 1;
            }
            other => visitor::walk_expr(self, other),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(inner) = handler;
        if let Some(name) = &inner.name {
            self.bind(name.as_str(), None);
        }
        visitor::walk_except_handler(self, handler);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        if let Some(name) = captured(pattern) {
            self.bind(name, None);
        }
        visitor::walk_pattern(self, pattern);
    }
}
