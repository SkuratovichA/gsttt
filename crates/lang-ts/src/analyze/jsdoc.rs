//! What the `JSDoc` comments of a JavaScript file say about types. The compiler reads them as types in a `.js` file, so a parameter documented `@param {Linter} linter` is a `Linter` and `linter.verifyAndFix()` is a call of that method. The scanner reads only the tags that carry a type, by hand and without a second parse, and a comment with no `@` costs nothing.

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::ast::Program;

use super::roots::is_class_like;

/// The tags whose `{type}` may name modules with `import('x')`.
const TYPED_TAGS: &[&str] = &[
    "param",
    "arg",
    "argument",
    "type",
    "returns",
    "return",
    "typedef",
    "property",
    "prop",
    "this",
    "callback",
    "template",
    "implements",
    "extends",
    "augments",
];

/// Names that are written like a class in `JSDoc` and are not one of the file: the global objects a value is a plain instance of.
const NOT_CLASSES: &[&str] = &[
    "Object", "Function", "Array", "Promise", "String", "Number", "Boolean", "Symbol", "BigInt",
];

/// The types that the `JSDoc` comments of one file give.
#[derive(Debug, Default)]
pub(crate) struct Docs {
    /// The comments that type a parameter or a value, by the start of the token they sit above.
    blocks: Vec<Block>,
    /// The type-only imports the comments make, sorted, once each.
    pub(super) imports: Vec<DocImport>,
}

/// The types one comment gives to the declaration under it.
#[derive(Debug)]
pub(super) struct Block {
    attached: u32,
    params: Vec<Param>,
    declared: Option<Box<str>>,
    /// The classes the tags `@param`, `@type`, `@returns`, `@return` and `@this` name: the declaration under the comment uses them.
    used: Vec<Box<str>>,
}

/// One `@param {T} name`, with the class of `T`.
#[derive(Debug)]
struct Param {
    /// The last segment of the documented name: `linter` for `config.linter`.
    name: Box<str>,
    /// Whether the documented name is a path into an object, which types a name inside a destructuring pattern and no plain parameter.
    dotted: bool,
    class: Box<str>,
}

/// A type-only import that a comment makes.
#[derive(Debug)]
pub(super) struct DocImport {
    pub(super) spec: String,
    pub(super) names: Vec<String>,
    /// The locals the names are bound to, as `Import::exported`.
    pub(super) exported: Vec<String>,
    /// The `(local, name)` pairs to bind, which a type position gives without exporting them.
    pub(super) binds: Vec<(String, String)>,
    /// Where it is: the offset of the first tag that makes it, or, for an import with no local, the offset of the token the comment sits above.
    pub(super) at: u32,
    /// The tokens that the comments making an `import('x').Y` sit above. The type binds `Y`, unless the code binds it: then each of these has an import with no local.
    pub(super) anchors: Vec<u32>,
}

impl Docs {
    /// What the `JSDoc` comments of `program`, written as `text`, say about types; `None` when they say nothing.
    pub(super) fn read(program: &Program<'_>, text: &str) -> Option<Self> {
        let mut reader = Reader::default();
        for comment in &program.comments {
            if !comment.is_jsdoc() {
                continue;
            }
            let span = comment.content_span();
            let range = usize::try_from(span.start).ok()?..usize::try_from(span.end).ok()?;
            if let Some(content) = text.get(range)
                && content.contains('@')
            {
                reader.comment(content, span.start, comment.attached_to);
            }
        }
        reader.finish()
    }

    /// The names that the `@import` tags and `import()` types of the file bind.
    pub(super) fn locals(&self) -> impl Iterator<Item = &str> {
        self.imports
            .iter()
            .flat_map(|doc| doc.binds.iter().map(|(local, _)| local.as_str()))
    }

    /// The comment that sits above the token that starts at `at`.
    pub(super) fn block_at(&self, at: u32) -> Option<&Block> {
        let after = self.blocks.partition_point(|block| block.attached <= at);
        self.blocks
            .get(after.checked_sub(1)?)
            .filter(|block| block.attached == at)
    }
}

impl Block {
    /// Whether the comment documents a parameter.
    pub(super) fn has_params(&self) -> bool {
        !self.params.is_empty()
    }

    /// The class that the parameter binding `name` holds. A name inside a destructuring pattern is typed by a path (`config.linter`), a plain parameter by its own name only.
    pub(super) fn param_class(&self, name: &str, destructured: bool) -> Option<&str> {
        self.params
            .iter()
            .find(|param| &*param.name == name && (destructured || !param.dotted))
            .map(|param| &*param.class)
    }

    /// The classes that the types of the comment name.
    pub(super) fn used(&self) -> impl Iterator<Item = &str> {
        self.used.iter().map(|class| &**class)
    }

    /// The class of the first `@type` of the comment.
    pub(super) fn declared_class(&self) -> Option<&str> {
        self.declared.as_deref()
    }
}

/// The tag being read: its name, the offset of its `@` and its text to the next tag, without the stars that start the lines.
struct Tag<'c> {
    name: &'c str,
    at: u32,
    body: String,
}

/// The spec, the names and the locals of an import.
/// and, for an import with no local, the token it is made above, so that two declarations that document the same module both have one.
type ImportKey = (String, Vec<String>, Vec<String>, u32);

/// What the tags that make one import say: the offset of the first, the pairs to bind and the tokens they are above.
#[derive(Default)]
struct Made {
    at: u32,
    binds: BTreeSet<(String, String)>,
    anchors: BTreeSet<u32>,
}

#[derive(Default)]
struct Reader {
    blocks: Vec<Block>,
    imports: BTreeMap<ImportKey, Made>,
    /// The token that the comment being read sits above, or the start of its first tag when it sits above none.
    anchor: u32,
}

impl Reader {
    fn finish(self) -> Option<Docs> {
        let imports: Vec<DocImport> = self
            .imports
            .into_iter()
            .map(|((spec, names, exported, _), made)| DocImport {
                spec,
                names,
                exported,
                binds: made.binds.into_iter().collect(),
                at: made.at,
                anchors: made.anchors.into_iter().collect(),
            })
            .collect();
        (!self.blocks.is_empty() || !imports.is_empty()).then_some(Docs {
            blocks: self.blocks,
            imports,
        })
    }

    /// Reads the tags of one comment, `content` being the text between `/*` and `*/` which starts at `start`.
    fn comment(&mut self, content: &str, start: u32, attached: u32) {
        let mut block = Block {
            attached,
            params: Vec::new(),
            declared: None,
            used: Vec::new(),
        };
        self.anchor = if attached == 0 { start } else { attached };
        let mut tag: Option<Tag<'_>> = None;
        let mut offset = 0;
        for line in content.split_inclusive('\n') {
            let text = line.trim_start_matches([' ', '\t', '*']);
            if let Some(rest) = text.strip_prefix('@') {
                if let Some(done) = tag.take() {
                    self.tag(&done, &mut block);
                }
                let at = start.saturating_add(position(offset + line.len() - text.len()));
                let (name, body) = split_at_end(rest, |c| !c.is_ascii_alphabetic());
                if name == "import" || TYPED_TAGS.contains(&name) {
                    tag = Some(Tag {
                        name,
                        at,
                        body: body.to_owned(),
                    });
                }
            } else if let Some(open) = tag.as_mut() {
                open.body.push('\n');
                open.body.push_str(text);
            }
            offset += line.len();
        }
        if let Some(done) = tag {
            self.tag(&done, &mut block);
        }
        if !block.params.is_empty() || block.declared.is_some() || !block.used.is_empty() {
            self.blocks.push(block);
        }
    }

    fn tag(&mut self, tag: &Tag<'_>, block: &mut Block) {
        if tag.name == "import" {
            self.import_tag(tag);
            return;
        }
        let Some((ty, after)) = braced(&tag.body) else {
            return;
        };
        let whole = whole_import(ty);
        if tag.name == "typedef"
            && let (Some((spec, name)), Some(alias)) = (whole, identifier(after))
        {
            // `import('x')` is the value that `x` exports, which `import Z from 'x'` takes.
            let name = name.unwrap_or("default");
            self.add(
                spec,
                vec![name.to_owned()],
                vec![alias.to_owned()],
                &[(alias, name)],
                tag.at,
                0,
            );
            return;
        }
        for (spec, name) in imports_in(ty) {
            if let Some(name) = name {
                self.qualified(spec, name, tag.at);
            } else {
                let anchor = self.anchor;
                self.add(
                    spec,
                    vec!["default".to_owned()],
                    Vec::new(),
                    &[],
                    anchor,
                    anchor,
                );
            }
        }
        let Some(class) = type_class(ty) else {
            return;
        };
        match tag.name {
            "param" | "arg" | "argument" => {
                block.used.push(class.into());
                if let Some(documented) = param_name(after) {
                    let (name, dotted) = match documented.rsplit_once('.') {
                        Some((_, last)) => (last, true),
                        None => (documented, false),
                    };
                    block.params.push(Param {
                        name: name.into(),
                        dotted,
                        class: class.into(),
                    });
                }
            }
            "type" => {
                block.used.push(class.into());
                if block.declared.is_none() {
                    block.declared = Some(class.into());
                }
            }
            "returns" | "return" | "this" => block.used.push(class.into()),
            _ => {}
        }
    }

    /// `@import { A, B as C } from "x"`, `@import * as ns from "x"` and `@import D from "x"`.
    fn import_tag(&mut self, tag: &Tag<'_>) {
        let Some((specifiers, spec)) = import_clause(&tag.body) else {
            return;
        };
        let names = specifiers.iter().map(|(name, _)| (*name).to_owned());
        let exported = specifiers.iter().map(|(_, local)| (*local).to_owned());
        let binds: Vec<(&str, &str)> = specifiers
            .iter()
            .map(|&(name, local)| (local, name))
            .collect();
        self.add(spec, names.collect(), exported.collect(), &binds, tag.at, 0);
    }

    /// `import('x').Y` in a tag: it binds `Y` to the name `Y` of `x`, so that a member of it resolves.
    fn qualified(&mut self, spec: &str, name: &str, at: u32) {
        let anchor = self.anchor;
        self.add(
            spec,
            vec![name.to_owned()],
            vec![name.to_owned()],
            &[(name, name)],
            at,
            0,
        );
        let key = (
            spec.to_owned(),
            vec![name.to_owned()],
            vec![name.to_owned()],
            0,
        );
        if let Some(made) = self.imports.get_mut(&key) {
            made.anchors.insert(anchor);
        }
    }

    fn add(
        &mut self,
        spec: &str,
        names: Vec<String>,
        exported: Vec<String>,
        binds: &[(&str, &str)],
        at: u32,
        anchor: u32,
    ) {
        let entry = self
            .imports
            .entry((spec.to_owned(), names, exported, anchor))
            .or_insert_with(|| Made {
                at,
                ..Made::default()
            });
        entry.at = entry.at.min(at);
        entry.binds.extend(
            binds
                .iter()
                .map(|&(local, name)| (local.to_owned(), name.to_owned())),
        );
    }
}

fn position(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// `text` cut before its first char that `stop` says so of, or whole.
fn split_at_end(text: &str, stop: impl Fn(char) -> bool) -> (&str, &str) {
    let end = text.find(stop).unwrap_or(text.len());
    text.split_at_checked(end).unwrap_or((text, ""))
}

/// The name that starts `text`, when it starts with one.
fn identifier(text: &str) -> Option<&str> {
    let name = split_at_end(text.trim_start(), |c| !is_ident(c)).0;
    (!name.is_empty()).then_some(name)
}

/// The text inside the braces that `body` starts with, which may nest, and what follows the closing brace.
fn braced(body: &str) -> Option<(&str, &str)> {
    let inner = body.trim_start().strip_prefix('{')?;
    let mut depth = 0usize;
    for (at, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => {
                let (ty, rest) = inner.split_at_checked(at)?;
                return Some((ty, rest.strip_prefix('}')?));
            }
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The name that a `@param` documents: `a` in `a`, `[a]` and `[a=1]`, `config.linter` in `config.linter`. A name with anything but names and dots in it is none.
fn param_name(after: &str) -> Option<&str> {
    let after = after.trim_start();
    let name = match after.strip_prefix('[') {
        Some(optional) => split_at_end(optional, |c| c == ']' || c == '=' || c.is_whitespace()).0,
        None => split_at_end(after, char::is_whitespace).0,
    };
    let valid = !name.is_empty()
        && name
            .split('.')
            .all(|segment| !segment.is_empty() && segment.chars().all(is_ident));
    valid.then_some(name)
}

/// `import('x')`, `import ("x")` and `import('x').Y` at the start of `text`: the specifier, the name after it and the text that follows.
fn import_at(text: &str) -> Option<(&str, Option<&str>, &str)> {
    let call = text
        .strip_prefix("import")?
        .trim_start()
        .strip_prefix('(')?
        .trim_start();
    let quote = call
        .chars()
        .next()
        .filter(|c| matches!(c, '\'' | '"' | '`'))?;
    let (spec, after) = call.strip_prefix(quote)?.split_once(quote)?;
    let after = after.trim_start().strip_prefix(')')?;
    if spec.is_empty() {
        return None;
    }
    match after.strip_prefix('.') {
        Some(qualified) => {
            let (name, rest) = split_at_end(qualified, |c| !is_ident(c));
            (!name.is_empty()).then_some((spec, Some(name), rest))
        }
        None => Some((spec, None, after)),
    }
}

/// Every module that `ty` imports, with the name it takes from it.
fn imports_in(ty: &str) -> Vec<(&str, Option<&str>)> {
    ty.match_indices("import")
        .filter(|&(at, _)| {
            ty.get(..at)
                .and_then(|before| before.chars().next_back())
                .is_none_or(|c| !is_ident(c) && c != '.')
        })
        .filter_map(|(at, _)| {
            let (spec, name, _) = import_at(ty.get(at..)?)?;
            Some((spec, name))
        })
        .collect()
}

/// The module and name of `ty` when the whole type is one import, `import('x').Y`.
fn whole_import(ty: &str) -> Option<(&str, Option<&str>)> {
    let (spec, name, rest) = import_at(peel(ty)?)?;
    rest.trim().is_empty().then_some((spec, name))
}

/// The members of a union, split at the `|` that are not inside brackets.
fn members(ty: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut from) = (0usize, 0);
    for (at, c) in ty.char_indices() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                parts.extend(ty.get(from..at));
                from = at + 1;
            }
            _ => {}
        }
    }
    parts.extend(ty.get(from..));
    parts
}

/// Whether the brackets of `text` close each other and none is left open.
fn is_balanced(text: &str) -> bool {
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => match depth.checked_sub(1) {
                Some(rest) => depth = rest,
                None => return false,
            },
            _ => {}
        }
    }
    depth == 0
}

/// `ty` without what does not change the class of a value: one leading `?` or `!`, a trailing `=`, parentheses around the whole and the `null` and `undefined` members of a union. `None` when two members are left.
fn peel(ty: &str) -> Option<&str> {
    let mut ty = ty;
    loop {
        let text = ty.trim();
        let text = text.strip_prefix(['?', '!']).unwrap_or(text);
        let text = text.strip_suffix('=').unwrap_or(text).trim();
        if let Some(inner) = text
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'))
            .filter(|inner| is_balanced(inner))
        {
            ty = inner;
            continue;
        }
        let parts = members(text);
        if parts.len() < 2 {
            return Some(text);
        }
        let mut left = parts
            .into_iter()
            .filter(|member| !matches!(member.trim(), "null" | "undefined"));
        let only = left.next()?;
        if left.next().is_some() {
            return None;
        }
        ty = only;
    }
}

/// The class that a `JSDoc` type stands for: a dotted name that starts with an upper-case letter, `Linter` or `Orders.Service`, or `import('x').Y`, which is `Y`, also behind `?`, `!`, `=`, parentheses and `|null`. A union of two classes, a generic, an array and the global objects stand for none.
pub(super) fn type_class(ty: &str) -> Option<&str> {
    let ty = peel(ty)?;
    if let Some((_, name, rest)) = import_at(ty) {
        return name.filter(|_| rest.trim().is_empty());
    }
    let named = is_class_like(ty)
        && ty
            .split('.')
            .all(|segment| !segment.is_empty() && segment.chars().all(is_ident));
    (named && !NOT_CLASSES.contains(&ty)).then_some(ty)
}

/// `{ A, B as C }`, `* as ns`, `D` and their mixes, then `from "x"`: the `(name, local)` pairs and the specifier.
fn import_clause(body: &str) -> Option<(Vec<(&str, &str)>, &str)> {
    let mut specifiers = Vec::new();
    let mut rest = body.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("from")
            && after.starts_with(|c: char| c.is_whitespace() || matches!(c, '\'' | '"'))
        {
            rest = after.trim_start();
            break;
        }
        if let Some(list) = rest.strip_prefix('{') {
            let (list, after) = list.split_once('}')?;
            for item in list.split(',') {
                let mut words = item.split_whitespace().filter(|word| *word != "type");
                let Some(name) = words.next() else {
                    continue;
                };
                let local = match (words.next(), words.next()) {
                    (Some("as"), Some(local)) => local,
                    _ => name,
                };
                specifiers.push((name, local));
            }
            rest = after;
        } else if let Some(star) = rest.strip_prefix('*') {
            let alias = star.trim_start().strip_prefix("as")?;
            let (local, after) = split_at_end(alias.trim_start(), |c| !is_ident(c));
            if local.is_empty() {
                return None;
            }
            specifiers.push(("*", local));
            rest = after;
        } else {
            let (local, after) = split_at_end(rest, |c| !is_ident(c));
            if local.is_empty() {
                return None;
            }
            specifiers.push(("default", local));
            rest = after;
        }
        rest = rest.trim_start().trim_start_matches(',').trim_start();
    }
    let quote = rest.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    let (spec, _) = rest.strip_prefix(quote)?.split_once(quote)?;
    (!spec.is_empty() && !specifiers.is_empty()).then_some((specifiers, spec))
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;
    use proptest::prelude::*;

    use super::super::roots::plain_class;
    use super::*;

    /// The class that the TypeScript reading of `let x: <ty>` gives.
    fn typescript_class(ty: &str) -> Option<String> {
        let allocator = Allocator::default();
        let source = format!("let x: {ty};");
        let parsed = Parser::new(&allocator, &source, SourceType::ts()).parse();
        let oxc_ast::ast::Statement::VariableDeclaration(declaration) =
            parsed.program.body.first()?
        else {
            return None;
        };
        let annotation = declaration
            .declarations
            .first()?
            .type_annotation
            .as_deref()?;
        plain_class(annotation).map(std::borrow::Cow::into_owned)
    }

    fn docs_of(source: &str) -> Option<Docs> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
        Docs::read(&parsed.program, source)
    }

    /// The second copy of what `plain_type_class` answers, which reads a syntax tree where this one reads a string: both answer alike on the types they share.
    #[test]
    fn a_type_gives_the_class_a_typescript_annotation_gives() {
        for ty in [
            "Linter",
            "Orders.Service",
            "Foo<Bar>",
            "Foo[]",
            "A | B",
            "string",
            "{ a: number }",
        ] {
            assert_eq!(
                type_class(ty).map(str::to_owned),
                typescript_class(ty).filter(|class| is_class_like(class) && !class.contains('<')),
                "{ty}"
            );
        }
    }

    #[test]
    fn a_type_is_a_class_only_when_it_is_one_plain_class() {
        for (ty, class) in [
            ("Linter", Some("Linter")),
            ("?Linter", Some("Linter")),
            ("!Linter=", Some("Linter")),
            ("(Linter|null)", Some("Linter")),
            ("Linter|undefined", Some("Linter")),
            ("import('x').Y", Some("Y")),
            ("import ('x').Y | null", Some("Y")),
            ("import('x')", None),
            ("import('x').Y.Z", None),
            ("A|B", None),
            ("Foo<Bar>", None),
            ("Foo[]", None),
            ("linter", None),
            ("Object", None),
            ("Function", None),
            ("Array", None),
            ("Promise", None),
            ("null", None),
            ("", None),
        ] {
            assert_eq!(type_class(ty), class, "{ty}");
        }
    }

    #[test]
    fn the_tags_of_a_comment_are_read() {
        let docs = docs_of(
            "/**\n * @param {import ('./a').Sel} a\n * @param {?Linter} [b=1]\n * @param {Config} config.linter\n * @example\n * @param {import('no')} ignored\n */\nfunction f(a, b) {}\n",
        )
        .unwrap();
        let block = docs.block_at(docs.blocks[0].attached).unwrap();
        assert_eq!(block.param_class("a", false), Some("Sel"));
        assert_eq!(block.param_class("b", false), Some("Linter"));
        assert_eq!(block.param_class("linter", false), None);
        assert_eq!(block.param_class("linter", true), Some("Config"));
    }

    proptest! {
        #[test]
        fn no_type_panics(ty in "\\PC{0,40}") {
            let _ = type_class(&ty);
            drop(imports_in(&ty));
            let _ = whole_import(&ty);
        }

        #[test]
        fn no_comment_panics(body in "[ -~\\n@{}()|*,'\"]{0,120}") {
            let source = format!("/**\n{body}\n */\nfunction f() {{}}");
            drop(docs_of(&source));
            let mut reader = Reader::default();
            reader.comment(&body, 0, 0);
            drop(import_clause(&body));
        }
    }
}
