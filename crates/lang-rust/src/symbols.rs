//! The declarations of a Rust file and what each of them uses, so that a question about a name is answered by the files that use that name and not by every file that imports the one that declares it.
//!
//! Nothing here expands a macro or resolves a type: a name is recorded as the source spells it, and the type of a receiver is known only where the file writes it down.

mod receivers;
mod table;

use std::collections::{BTreeSet, HashMap};

use gsttt_core::fact::{Declared, Heir, Import, ImportKind, Line, MEMBER_SEPARATOR, Symbols};
use gsttt_core::text::LineIndex;
use ra_ap_syntax::ast::{self, HasName, HasTypeBounds};
use ra_ap_syntax::{AstNode, SyntaxNode};

use crate::imports::{binding, chains, significant_tokens};
use crate::syntax::text;
use receivers::Scan;
use table::{Generics, Table, flatten, impl_target, last_segment, of_std};

// A use is written with the separator the core splits a member on, and a Rust path is written with `::`.
const _: () = assert!(MEMBER_SEPARATOR == '.');

/// What reading the code of a file gives.
#[derive(Debug)]
pub(crate) struct Read {
    pub(crate) symbols: Symbols,
    /// What an importer can name that the file declares or hands out with `pub use`, sorted: every top-level name, private ones too, because a module below the one that declares them reaches them with `use super::*`.
    pub(crate) exports: Vec<String>,
}

/// The names that a `use` binds and nothing in the file names, of no crate of the standard library. The compiler warns about an import nothing needs, so such an import is there for what it brings without its name: a trait whose methods the file calls, `as _` among them, or a macro.
fn unnamed_imports(
    imports: &[Import],
    entries: &[Entry],
    on_load: &BTreeSet<String>,
) -> Vec<String> {
    let named: BTreeSet<&str> = entries
        .iter()
        .flat_map(|entry| entry.uses.iter())
        .chain(on_load)
        .map(String::as_str)
        .collect();
    imports
        .iter()
        .filter(|import| import.kind == ImportKind::Static && !of_std(import))
        .filter_map(|import| match import.names.as_slice() {
            [taken] => Some(import.exported.first().unwrap_or(taken)),
            _ => None,
        })
        .filter(|bound| *bound != "*" && !named.contains(bound.as_str()))
        .cloned()
        .collect()
}

/// Reads the declarations of a file that parsed without error. `imports` are the ones the file's `use` items and its paths make.
pub(crate) fn read(root: &SyntaxNode, lines: &LineIndex, imports: &[Import]) -> Read {
    let mut items = Vec::new();
    flatten(root, &mut items);
    let table = Table::of(&items, imports);
    let mut recorder = Recorder::new(&table, lines);
    for item in &items {
        recorder.item(item);
    }
    recorder.variants_imported(imports);
    recorder.finish(imports)
}

pub(crate) struct Entry {
    name: String,
    line: Line,
    end: Line,
    pub(crate) uses: BTreeSet<String>,
}

pub(crate) struct Recorder<'a> {
    table: &'a Table,
    lines: &'a LineIndex,
    pub(crate) entries: Vec<Entry>,
    index: HashMap<String, usize>,
    on_load: BTreeSet<String>,
    pub(crate) reads: BTreeSet<String>,
    /// The members read on a value of a written type that the file does not declare for that type, with the declaration that reads them.
    pub(crate) written: BTreeSet<(usize, String)>,
    heirs: Vec<Heir>,
    overrides: Vec<Heir>,
}

impl<'a> Recorder<'a> {
    fn new(table: &'a Table, lines: &'a LineIndex) -> Self {
        Self {
            table,
            lines,
            entries: Vec::new(),
            index: HashMap::new(),
            on_load: BTreeSet::new(),
            reads: BTreeSet::new(),
            written: BTreeSet::new(),
            heirs: Vec::new(),
            overrides: Vec::new(),
        }
    }

    /// The declaration `name`, made or found, which spans `node`. Items of one name, such as the same method under two `cfg`s, share one entry.
    fn declare(&mut self, name: String, node: &SyntaxNode) -> usize {
        let range = node.text_range();
        let start = u32::from(range.start());
        let last = u32::from(range.end()).saturating_sub(1).max(start);
        let (line, end) = (self.lines.line(start), self.lines.line(last));
        if let Some(&position) = self.index.get(&name) {
            if let Some(entry) = self.entries.get_mut(position) {
                entry.line = entry.line.min(line);
                entry.end = entry.end.max(end);
            }
            return position;
        }
        let position = self.entries.len();
        self.index.insert(name.clone(), position);
        self.entries.push(Entry {
            name,
            line,
            end,
            uses: BTreeSet::new(),
        });
        position
    }

    /// Reads what `node` uses into the declaration at `owner`. `this` is what `Self` and `self` are in it, and `skip` a part of the node that belongs to other declarations.
    fn scan(
        &mut self,
        owner: usize,
        node: &SyntaxNode,
        this: Option<&str>,
        outer: &Generics,
        skip: Option<&SyntaxNode>,
    ) {
        let mut scan = Scan::new(self, owner, this, outer);
        scan.run(node, skip);
    }

    fn heir(&mut self, name: &str, of: &str, dispatch: bool) {
        let heir = Heir {
            name: name.to_owned(),
            of: of.to_owned(),
        };
        if dispatch {
            self.overrides.push(heir.clone());
        }
        self.heirs.push(heir);
    }

    fn item(&mut self, item: &ast::Item) {
        let none = Generics::default();
        match item {
            ast::Item::Fn(function) => {
                if let Some(name) = function.name() {
                    let owner = self.declare(text(&name), function.syntax());
                    self.scan(owner, function.syntax(), None, &none, None);
                }
            }
            ast::Item::Struct(item) => {
                let Some(name) = item.name().map(|name| text(&name)) else {
                    return;
                };
                let owner = self.declare(name.clone(), item.syntax());
                self.scan(owner, item.syntax(), Some(&name), &none, None);
                self.derives(item.syntax(), &name);
                if let Some(ast::FieldList::RecordFieldList(fields)) = item.field_list() {
                    self.fields(&name, &fields);
                }
            }
            ast::Item::Union(item) => {
                let Some(name) = item.name().map(|name| text(&name)) else {
                    return;
                };
                let owner = self.declare(name.clone(), item.syntax());
                self.scan(owner, item.syntax(), Some(&name), &none, None);
                self.derives(item.syntax(), &name);
                if let Some(fields) = item.record_field_list() {
                    self.fields(&name, &fields);
                }
            }
            ast::Item::Enum(item) => {
                let Some(name) = item.name().map(|name| text(&name)) else {
                    return;
                };
                let owner = self.declare(name.clone(), item.syntax());
                self.scan(owner, item.syntax(), Some(&name), &none, None);
                self.derives(item.syntax(), &name);
                for variant in item
                    .variant_list()
                    .iter()
                    .flat_map(ast::VariantList::variants)
                {
                    if let Some(variant_name) = variant.name() {
                        let member = member_name(&name, &text(&variant_name));
                        let owner = self.declare(member, variant.syntax());
                        self.scan(owner, variant.syntax(), Some(&name), &none, None);
                    }
                }
            }
            ast::Item::Trait(item) => self.trait_item(item),
            ast::Item::Impl(item) => self.impl_item(item),
            ast::Item::TypeAlias(item) => {
                if let Some(name) = item.name().map(|name| text(&name)) {
                    let owner = self.declare(name.clone(), item.syntax());
                    self.scan(owner, item.syntax(), None, &none, None);
                    self.held(&name, item.ty().as_ref());
                }
            }
            ast::Item::Const(item) => {
                let name = item.name().map(|name| text(&name));
                self.value(name, item.syntax(), item.ty().as_ref());
            }
            ast::Item::Static(item) => {
                let name = item.name().map(|name| text(&name));
                self.value(name, item.syntax(), item.ty().as_ref());
            }
            ast::Item::MacroRules(item) => {
                if let Some(name) = item.name() {
                    let owner = self.declare(text(&name), item.syntax());
                    self.scan(owner, item.syntax(), None, &none, None);
                }
            }
            _ => {}
        }
    }

    /// A `const` or a `static`. `const _` declares no name: it runs as the file loads, so what it uses is what loading the file uses.
    fn value(&mut self, name: Option<String>, node: &SyntaxNode, ty: Option<&ast::Type>) {
        let none = Generics::default();
        match name {
            Some(name) if name != "_" => {
                let owner = self.declare(name.clone(), node);
                self.scan(owner, node, None, &none, None);
                self.held(&name, ty);
            }
            _ => {
                // The uses of an anonymous constant belong to no declaration, so a throwaway one collects them.
                let owner = self.declare("_".to_owned(), node);
                self.scan(owner, node, None, &none, None);
                if let Some(entry) = self.entries.get_mut(owner) {
                    let uses = std::mem::take(&mut entry.uses);
                    self.on_load.extend(uses);
                }
            }
        }
    }

    /// A name that holds a value of a type takes the members of the type: a `static X: Foo` runs the methods of `Foo`.
    fn held(&mut self, name: &str, ty: Option<&ast::Type>) {
        if let Some(head) = ty.and_then(|ty| self.table.class_of(ty, &Generics::default())) {
            self.heir(name, &head, false);
        }
    }

    /// `S.f` for each field of a struct.
    fn fields(&mut self, name: &str, fields: &ast::RecordFieldList) {
        let none = Generics::default();
        for field in fields.fields() {
            if let Some(field_name) = field.name() {
                let owner = self.declare(member_name(name, &text(&field_name)), field.syntax());
                self.scan(owner, field.syntax(), Some(name), &none, None);
            }
        }
    }

    /// `#[derive(Trait)]` makes the type an heir of the trait: the methods of the trait run on it.
    fn derives(&mut self, node: &SyntaxNode, name: &str) {
        for attr in node.children().filter_map(ast::Attr::cast) {
            for derived in derive_names(&attr) {
                self.heir(name, &derived, true);
            }
        }
    }

    fn trait_item(&mut self, item: &ast::Trait) {
        let Some(name) = item.name().map(|name| text(&name)) else {
            return;
        };
        let none = Generics::default();
        let list = item.assoc_item_list();
        let owner = self.declare(name.clone(), item.syntax());
        self.scan(
            owner,
            item.syntax(),
            Some(&name),
            &none,
            list.as_ref().map(AstNode::syntax),
        );
        for bound in item
            .type_bound_list()
            .iter()
            .flat_map(ast::TypeBoundList::bounds)
        {
            if let Some(head) = bound.ty().and_then(|ty| last_segment(&ty)) {
                self.heir(&name, &head, false);
            }
        }
        for member in list.iter().flat_map(ast::AssocItemList::assoc_items) {
            if let Some(label) = assoc_name(&member) {
                let owner = self.declare(member_name(&name, &label), member.syntax());
                self.scan(owner, member.syntax(), Some(&name), &none, None);
            }
        }
    }

    fn impl_item(&mut self, item: &ast::Impl) {
        let generics = Generics::of(item);
        let trait_head = item.trait_().and_then(|ty| last_segment(&ty));
        // A blanket `impl<T> Trait for T` has no type to name its members after, so they go under the trait.
        let target = impl_target(item, &generics).or_else(|| trait_head.clone());
        let Some(target) = target else {
            return;
        };
        if let (Some(head), Some(of)) = (impl_target(item, &generics), trait_head) {
            self.heir(&head, &of, true);
        }
        let mut owners = Vec::new();
        for member in item
            .assoc_item_list()
            .iter()
            .flat_map(ast::AssocItemList::assoc_items)
        {
            if let Some(label) = assoc_name(&member) {
                let owner = self.declare(member_name(&target, &label), member.syntax());
                self.scan(owner, member.syntax(), Some(&target), &generics, None);
                owners.push(owner);
            }
        }
        // The header names the type and the trait: whoever uses either is reached by the block, in whichever file it stands. An empty block is declared under the trait so that it is not lost.
        if owners.is_empty() {
            let label = trait_head_name(item).unwrap_or_else(|| "impl".to_owned());
            owners.push(self.declare(member_name(&target, &label), item.syntax()));
        }
        let Some(&first) = owners.first() else {
            return;
        };
        let before = self.entries.get(first).map(|entry| entry.uses.clone());
        if let Some(list) = item.assoc_item_list() {
            self.scan(
                first,
                item.syntax(),
                Some(&target),
                &generics,
                Some(list.syntax()),
            );
        } else {
            self.scan(first, item.syntax(), Some(&target), &generics, None);
        }
        let header: Vec<String> = match (self.entries.get(first), before) {
            (Some(entry), Some(before)) => entry.uses.difference(&before).cloned().collect(),
            _ => Vec::new(),
        };
        for &owner in owners.iter().skip(1) {
            if let Some(entry) = self.entries.get_mut(owner) {
                entry.uses.extend(header.iter().cloned());
            }
        }
    }

    /// `use a::Color::Red` reads `Red`, a variant or an associated item the file names bare: it uses `Red` of whatever class the file holds.
    fn variants_imported(&mut self, imports: &[Import]) {
        for import in imports {
            if !matches!(import.kind, ImportKind::Static | ImportKind::ReExport) {
                continue;
            }
            let ([taken], [bound]) = (import.names.as_slice(), import.exported.as_slice()) else {
                continue;
            };
            let leaf = import.spec.rsplit("::").next().unwrap_or_default();
            if bound != taken && leaf != taken && leaf != "*" {
                self.reads.insert(leaf.to_owned());
            }
        }
    }

    fn finish(self, imports: &[Import]) -> Read {
        let mut holds: Vec<Heir> = self
            .table
            .holders()
            .map(|(name, of)| Heir {
                name: name.to_owned(),
                of: of.to_owned(),
            })
            .collect();
        let Self {
            mut entries,
            mut on_load,
            mut reads,
            written,
            mut heirs,
            mut overrides,
            ..
        } = self;
        let unnamed = unnamed_imports(imports, &entries, &on_load);
        // The file holds such an import for its methods, so a member a value of a written type does not declare here may be one of them.
        if !unnamed.is_empty() {
            for (owner, member) in written {
                if let Some(entry) = entries.get_mut(owner) {
                    entry.uses.insert(format!("{MEMBER_SEPARATOR}{member}"));
                }
                reads.insert(member);
            }
        }
        on_load.extend(unnamed);
        let mut exports: BTreeSet<String> = entries
            .iter()
            .filter(|entry| !entry.name.contains(MEMBER_SEPARATOR) && entry.name != "_")
            .map(|entry| entry.name.clone())
            .collect();
        exports.extend(
            imports
                .iter()
                .filter(|import| import.kind == ImportKind::ReExport)
                .filter_map(binding)
                .map(str::to_owned),
        );
        let mut declared: Vec<Declared> = entries
            .into_iter()
            .filter(|entry| entry.name != "_")
            .map(|entry| Declared {
                uses: entry
                    .uses
                    .into_iter()
                    .filter(|used| *used != entry.name)
                    .collect(),
                name: entry.name,
                line: entry.line,
                end: entry.end,
            })
            .collect();
        declared.sort_unstable_by(|a, b| (a.line, &a.name).cmp(&(b.line, &b.name)));
        heirs.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        heirs.dedup();
        overrides.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        overrides.dedup();
        holds.sort_unstable_by(|a, b| (&a.name, &a.of).cmp(&(&b.name, &b.of)));
        holds.dedup();
        Read {
            symbols: Symbols {
                declared,
                on_load: on_load.into_iter().collect(),
                exports: Vec::new(),
                heirs,
                overrides,
                holds,
                reads: reads.into_iter().collect(),
            },
            exports: exports.into_iter().collect(),
        }
    }
}

/// `class.member`, as a declaration lists a member.
fn member_name(class: &str, member: &str) -> String {
    format!("{class}{MEMBER_SEPARATOR}{member}")
}

/// The name an associated item declares.
fn trait_head_name(item: &ast::Impl) -> Option<String> {
    last_segment(&item.trait_()?)
}

pub(crate) fn assoc_name(item: &ast::AssocItem) -> Option<String> {
    let name = match item {
        ast::AssocItem::Fn(item) => item.name(),
        ast::AssocItem::Const(item) => item.name(),
        ast::AssocItem::TypeAlias(item) => item.name(),
        ast::AssocItem::MacroCall(_) => None,
    };
    name.map(|name| text(&name))
}

/// The last segment of each trait a `#[derive(..)]` names.
pub(crate) fn derive_names(attr: &ast::Attr) -> Vec<String> {
    if attr.simple_name().as_deref() != Some("derive") {
        return Vec::new();
    }
    chains(&significant_tokens(attr.syntax()))
        .into_iter()
        .filter_map(|chain| chain.names.last().cloned())
        .filter(|name| name != "derive")
        .collect()
}
