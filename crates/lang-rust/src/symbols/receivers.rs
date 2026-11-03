//! The reading of the code of one declaration: which names it uses, and on what a method it calls is called.

use std::collections::{HashMap, HashSet};

use gsttt_core::fact::MEMBER_SEPARATOR;
use ra_ap_syntax::ast::{self, HasName, PathSegmentKind};
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, WalkEvent};

use super::table::Generics;
use super::{Recorder, derive_names};
use crate::imports::{chains, significant_tokens};
use crate::syntax::text;

/// The reading of one declaration.
pub(super) struct Scan<'r, 'a> {
    rec: &'r mut Recorder<'a>,
    owner: usize,
    /// What `Self` and `self` are.
    this: Option<String>,
    /// The names the declaration binds with a pattern or a parameter, in the whole of it.
    locals: HashSet<String>,
    /// The type parameters of the declaration and of the block around it.
    generics: Generics,
    /// The class of each local bound with a type written down, `None` when it is bound twice to different ones or to none.
    classes: HashMap<String, Option<String>>,
}

impl<'r, 'a> Scan<'r, 'a> {
    pub(super) fn new(
        rec: &'r mut Recorder<'a>,
        owner: usize,
        this: Option<&str>,
        outer: &Generics,
    ) -> Self {
        Self {
            rec,
            owner,
            this: this.map(str::to_owned),
            locals: HashSet::new(),
            generics: outer.clone(),
            classes: HashMap::new(),
        }
    }

    pub(super) fn run(&mut self, node: &SyntaxNode, skip: Option<&SyntaxNode>) {
        self.bind(node);
        let mut events = node.preorder_with_tokens();
        while let Some(event) = events.next() {
            let WalkEvent::Enter(NodeOrToken::Node(inner)) = event else {
                continue;
            };
            if skip.is_some_and(|skip| *skip == inner) {
                events.skip_subtree();
                continue;
            }
            match inner.kind() {
                SyntaxKind::USE => events.skip_subtree(),
                SyntaxKind::ATTR => {
                    if let Some(attr) = ast::Attr::cast(inner) {
                        for name in derive_names(&attr) {
                            self.note(&name);
                        }
                    }
                    events.skip_subtree();
                }
                SyntaxKind::TOKEN_TREE => {
                    self.tokens(&inner);
                    events.skip_subtree();
                }
                SyntaxKind::PATH => {
                    if let Some(path) = ast::Path::cast(inner)
                        && path.syntax().parent().map(|parent| parent.kind())
                            != Some(SyntaxKind::PATH)
                    {
                        self.path(&path);
                    }
                }
                SyntaxKind::METHOD_CALL_EXPR => {
                    if let Some(call) = ast::MethodCallExpr::cast(inner) {
                        self.member(call.receiver().as_ref(), call.name_ref());
                    }
                }
                SyntaxKind::FIELD_EXPR => {
                    if let Some(field) = ast::FieldExpr::cast(inner) {
                        self.member(field.expr().as_ref(), field.name_ref());
                    }
                }
                SyntaxKind::IDENT_PAT => {
                    // A pattern of CamelCase is a constant or a variant, not a binding.
                    if let Some(pat) = ast::IdentPat::cast(inner)
                        && let Some(name) = pat.name().map(|name| text(&name))
                        && name.starts_with(char::is_uppercase)
                    {
                        self.note(&name);
                    }
                }
                SyntaxKind::RECORD_EXPR_FIELD => {
                    if let Some(field) = ast::RecordExprField::cast(inner) {
                        self.literal_field(&field);
                    }
                }
                _ => {}
            }
        }
    }

    fn note(&mut self, used: &str) {
        if let Some(entry) = self.rec.entries.get_mut(self.owner)
            && !entry.uses.contains(used)
        {
            entry.uses.insert(used.to_owned());
        }
    }

    /// Notes that the declaration reads a member `member` of a value whose class is not known: no class is named, and a file that holds a class with that member, or the trait, may be the one the call reaches.
    fn unknown(&mut self, member: &str) {
        self.note(&format!("{MEMBER_SEPARATOR}{member}"));
        if !self.rec.reads.contains(member) {
            self.rec.reads.insert(member.to_owned());
        }
    }

    /// Notes that the declaration uses `member` of `class`. A method or a field the file does not declare for the class may come from a trait in scope, which a call written with the class does not name: the recorder decides at the end of the file, when it knows whether one is.
    fn class_member(&mut self, class: &str, member: &str) {
        self.note(&format!("{class}{MEMBER_SEPARATOR}{member}"));
        let elsewhere = !self.rec.table.declares(class, member)
            && !self.rec.table.is_std(class)
            && member.starts_with(char::is_lowercase);
        if elsewhere {
            self.rec.written.insert((self.owner, member.to_owned()));
        }
    }

    /// The names the declaration binds: with a pattern, a parameter, a type parameter, and the class of those that are written with a type.
    fn bind(&mut self, node: &SyntaxNode) {
        // The parameters first, because the type of a pattern names them and a `where` comes after the patterns.
        for inner in node.descendants() {
            if let Some(param) = ast::TypeParam::cast(inner.clone()) {
                self.generics.add_type(&param);
            } else if let Some(param) = ast::ConstParam::cast(inner.clone()) {
                self.generics.add_const(&param);
            } else if let Some(pred) = ast::WherePred::cast(inner) {
                self.generics.add_where(&pred);
            }
        }
        for pat in node.descendants().filter_map(ast::IdentPat::cast) {
            let Some(name) = pat.name().map(|name| text(&name)) else {
                continue;
            };
            if name.starts_with(char::is_uppercase) {
                continue;
            }
            let class = self.class_written(&pat);
            self.locals.insert(name.clone());
            match self.classes.get_mut(&name) {
                Some(known) if *known != class => *known = None,
                Some(_) => {}
                None => {
                    self.classes.insert(name, class);
                }
            }
        }
    }

    /// The class of a binding whose type the code writes: `x: Foo` as a parameter or in a `let`, `let x = Foo { .. }`.
    fn class_written(&self, pat: &ast::IdentPat) -> Option<String> {
        let parent = pat.syntax().parent()?;
        let table = self.rec.table;
        if let Some(param) = ast::Param::cast(parent.clone()) {
            return table.class_of(&param.ty()?, &self.generics);
        }
        let stmt = ast::LetStmt::cast(parent)?;
        if let Some(ty) = stmt.ty() {
            return table.class_of(&ty, &self.generics);
        }
        match stmt.initializer()? {
            ast::Expr::RecordExpr(literal) => self.literal_class(&literal),
            _ => None,
        }
    }

    /// The class a struct literal makes: `Foo` for `Foo { .. }`, and what `Self` is for `Self { .. }`.
    fn literal_class(&self, literal: &ast::RecordExpr) -> Option<String> {
        let segment = literal.path()?.segment()?;
        let name = match segment.kind()? {
            PathSegmentKind::Name(name) => text(&name),
            PathSegmentKind::SelfTypeKw => self.this.clone()?,
            _ => return None,
        };
        name.starts_with(char::is_uppercase).then_some(name)
    }

    /// The class of the value of `expr`, where the code says so: `self`, a local with a written type, a field of those with a type written in the struct.
    fn class_of(&self, expr: &ast::Expr) -> Option<String> {
        match expr {
            ast::Expr::PathExpr(expr) => {
                let path = expr.path()?;
                if path.qualifier().is_some() {
                    return None;
                }
                match path.segment()?.kind()? {
                    PathSegmentKind::SelfKw => self.this.clone(),
                    PathSegmentKind::Name(name) => self.classes.get(&text(&name))?.clone(),
                    _ => None,
                }
            }
            ast::Expr::FieldExpr(field) => {
                let class = self.class_of(&field.expr()?)?;
                self.rec
                    .table
                    .field(&class, &text(&field.name_ref()?))
                    .cloned()
            }
            ast::Expr::ParenExpr(expr) => self.class_of(&expr.expr()?),
            ast::Expr::RefExpr(expr) => self.class_of(&expr.expr()?),
            ast::Expr::RecordExpr(literal) => self.literal_class(literal),
            _ => None,
        }
    }

    /// `receiver.member`, a method call or a field.
    fn member(&mut self, receiver: Option<&ast::Expr>, member: Option<ast::NameRef>) {
        let Some(member) = member.map(|name| text(&name)) else {
            return;
        };
        if member.starts_with(|ch: char| ch.is_ascii_digit()) {
            return;
        }
        // Recorded for a known class and an unknown one alike: the pair is about the names, and no use of a class.
        if let Some(on) = receiver.and_then(read_before) {
            self.rec
                .reads
                .insert(format!("{on}{MEMBER_SEPARATOR}{member}"));
        }
        match receiver.and_then(|receiver| self.class_of(receiver)) {
            Some(class) => self.class_member(&class, &member),
            None => self.unknown(&member),
        }
    }

    /// `field` in `Foo { field: value }`, a use of the field `Foo.field`.
    fn literal_field(&mut self, field: &ast::RecordExprField) {
        let literal = field.syntax().ancestors().find_map(ast::RecordExpr::cast);
        let Some(class) = literal.and_then(|literal| self.literal_class(&literal)) else {
            return;
        };
        if let Some(name) = field.field_name() {
            self.note(&format!("{class}{MEMBER_SEPARATOR}{}", text(&name)));
        }
    }

    /// A path written in code, such as `foo`, `Foo`, `Foo::bar`, `a::b::foo`, `Self::bar` and `<Foo as Tr>::bar`.
    fn path(&mut self, path: &ast::Path) {
        let callee = path
            .syntax()
            .parent()
            .and_then(|parent| ast::PathExpr::cast(parent)?.syntax().parent())
            .and_then(ast::CallExpr::cast)
            .is_some();
        let mut names = Vec::new();
        let mut qualifier: Option<(Option<String>, Option<String>)> = None;
        for segment in path.segments() {
            match segment.kind() {
                Some(PathSegmentKind::Name(name)) => names.push(text(&name)),
                Some(PathSegmentKind::SelfTypeKw) => {
                    names.push(self.this.clone().unwrap_or_else(|| "Self".to_owned()));
                }
                Some(PathSegmentKind::SelfKw) => names.push("self".to_owned()),
                Some(PathSegmentKind::SuperKw) => names.push("super".to_owned()),
                Some(PathSegmentKind::CrateKw) => names.push("crate".to_owned()),
                Some(PathSegmentKind::Type {
                    type_ref,
                    trait_ref,
                }) => {
                    let table = self.rec.table;
                    qualifier = Some((
                        type_ref.and_then(|ty| table.class_of(&ty, &self.generics)),
                        trait_ref.and_then(|ty| {
                            table.class_of(&ast::Type::PathType(ty), &self.generics)
                        }),
                    ));
                }
                None => return,
            }
        }
        match qualifier {
            Some((class, tr)) => {
                let member = names.first();
                for head in class.iter().chain(tr.iter()) {
                    match member {
                        Some(member) => self.note(&format!("{head}{MEMBER_SEPARATOR}{member}")),
                        None => self.note(head),
                    }
                }
            }
            None => self.named(&names, callee),
        }
    }

    /// The names of a path, outermost first.
    fn named(&mut self, names: &[String], callee: bool) {
        let Some(at) = names
            .iter()
            .position(|name| name.starts_with(char::is_uppercase))
        else {
            self.lower(names, callee);
            return;
        };
        let (Some(class), member) = (names.get(at), names.get(at + 1)) else {
            return;
        };
        // `T::m(x)` with `T: Tr` names the member of the trait, and the parameter alone names nothing.
        let class = match self.generics.bound(class) {
            Some(bound) if member.is_some() => bound.to_owned(),
            _ if self.generics.contains(class) => return,
            _ => class.clone(),
        };
        match member {
            Some(member) => self.note(&format!("{class}{MEMBER_SEPARATOR}{member}")),
            None => self.note(&class),
        }
    }

    /// A path with no name of CamelCase: a module path ends in what it names, and a name alone may be a local.
    fn lower(&mut self, names: &[String], callee: bool) {
        let Some(last) = names.last() else {
            return;
        };
        if matches!(last.as_str(), "self" | "super" | "crate") {
            return;
        }
        // `let config = config(..)`: a call names a function, whatever local has its name.
        if names.len() == 1 && self.locals.contains(last) && !callee {
            return;
        }
        self.note(last);
    }

    /// The arguments of a macro, which are not parsed: every path in them, the names read after a `.`, the name of a `$` variable skipped.
    fn tokens(&mut self, tree: &SyntaxNode) {
        let tokens = significant_tokens(tree);
        for token in &tokens {
            if token.kind() == SyntaxKind::STRING {
                for captured in captures(token.text()) {
                    self.named(&[captured], false);
                }
            }
        }
        for chain in chains(&tokens) {
            let before = chain
                .start
                .checked_sub(1)
                .and_then(|at| tokens.get(at))
                .map(SyntaxToken::kind);
            let single = match chain.names.as_slice() {
                [name] => Some(name),
                _ => None,
            };
            match (before, single) {
                (Some(SyntaxKind::DOLLAR), _) | (Some(SyntaxKind::DOT), None) => {}
                (Some(SyntaxKind::DOT), Some(member)) => {
                    let on_self = chain
                        .start
                        .checked_sub(2)
                        .and_then(|at| tokens.get(at))
                        .is_some_and(|token| token.kind() == SyntaxKind::SELF_KW);
                    match self.this.clone() {
                        Some(this) if on_self => self.class_member(&this, member),
                        _ => self.unknown(member),
                    }
                }
                _ => {
                    let names: Vec<String> = chain
                        .names
                        .iter()
                        .map(|name| match (name.as_str(), &self.this) {
                            ("Self", Some(this)) => this.clone(),
                            _ => name.clone(),
                        })
                        .collect();
                    self.named(&names, false);
                }
            }
        }
    }
}

/// The methods that give back the value they were called on, or the one inside it, so that a member read after them is read on what came before: `x.range().expect("..").min()` reads `min` on what `range` gives.
const HANDS_ON: &[&str] = &["unwrap", "expect", "as_ref", "as_mut", "as_deref", "clone"];

/// The name of the field or method that gives the value of `expr`, through parentheses, references, `?`, `.await` and the calls of `HANDS_ON`.
fn read_before(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::ParenExpr(expr) => read_before(&expr.expr()?),
        ast::Expr::RefExpr(expr) => read_before(&expr.expr()?),
        ast::Expr::TryExpr(expr) => read_before(&expr.expr()?),
        ast::Expr::AwaitExpr(expr) => read_before(&expr.expr()?),
        ast::Expr::FieldExpr(field) => {
            let name = text(&field.name_ref()?);
            (!name.starts_with(|ch: char| ch.is_ascii_digit())).then_some(name)
        }
        ast::Expr::MethodCallExpr(call) => {
            let name = text(&call.name_ref()?);
            if HANDS_ON.contains(&name.as_str()) {
                read_before(&call.receiver()?)
            } else {
                Some(name)
            }
        }
        _ => None,
    }
}

/// The names a format string captures: `{TAB}` and `{value:?}` in `"..{TAB}.."`. `{{` is a brace, not a capture.
fn captures(literal: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut chars = literal.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '{' {
            continue;
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            continue;
        }
        let mut name = String::new();
        while let Some(&next) = chars.peek() {
            if !(next.is_alphanumeric() || next == '_') {
                break;
            }
            name.push(next);
            chars.next();
        }
        let ends = matches!(chars.peek(), Some('}' | ':'));
        let starts = name.starts_with(|first: char| first.is_alphabetic() || first == '_');
        if ends && starts {
            found.push(name);
        }
    }
    found
}
