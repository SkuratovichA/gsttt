//! What a member of a class may be run through: the same member of each class or interface it implements or extends, which a caller may hold in place of the class.

use std::collections::BTreeSet;

use crate::fact::{ImportKind, MEMBER_SEPARATOR, ResolvedImport, member_in};
use crate::ids::{FileId, Id};
use crate::index::Index;

use super::exports::star_targets;

/// What the resolution of a name has been through, so that a cycle of imports or of `export *` ends. The flag tells a name looked up as the file's own from one looked up as its export.
type Seen<'a> = BTreeSet<(FileId, &'a str, bool)>;

/// The declarations the member `name` of a class declared in `file` may be run through: the same member of each head of the class, found in the file that declares the head, and of the heads of those in turn. Each as its file and its place among the declarations of the file, sorted. A head the repository does not hold, or that does not declare the member, gives nothing.
pub(super) fn overridden(index: &Index, file: FileId, name: &str) -> Vec<(FileId, usize)> {
    let Some((class, member)) = name.split_once(MEMBER_SEPARATOR) else {
        return Vec::new();
    };
    // A private member belongs to its class alone.
    if member.starts_with('#') {
        return Vec::new();
    }
    let mut found = BTreeSet::new();
    let mut visited = BTreeSet::from([(file, class)]);
    let mut work = vec![(file, class)];
    while let Some((at, class)) = work.pop() {
        let Some(symbols) = index.graph.symbols(at) else {
            continue;
        };
        for (_, of) in symbols.overrides().filter(|&(heir, _)| heir == class) {
            let Some((owner, head)) = declaring(index, at, of) else {
                continue;
            };
            if !visited.insert((owner, head)) {
                continue;
            }
            let place = index.graph.symbols(owner).and_then(|symbols| {
                symbols
                    .declarations()
                    .position(|decl| member_in(decl.name(), head) == Some(member))
            });
            found.extend(place.map(|place| (owner, place)));
            work.push((owner, head));
        }
    }
    found.into_iter().collect()
}

/// The declarations that override the member at `place` in `file`: the same member of each class that extends or implements its class, at any depth, each as its file and its place, sorted. It is [`overridden`] read from the other side, so the two cannot disagree about what is a head.
pub(super) fn overriding(index: &Index, file: FileId, place: usize) -> Vec<(FileId, usize)> {
    let Some(name) = index
        .graph
        .symbols(file)
        .and_then(|symbols| symbols.declaration(place))
        .map(crate::graph::Declaration::name)
    else {
        return Vec::new();
    };
    let Some((_, member)) = name.split_once(MEMBER_SEPARATOR) else {
        return Vec::new();
    };
    // A value is not called through a constructor.
    if matches!(member, "constructor" | "__init__") {
        return Vec::new();
    }
    let mut found = BTreeSet::new();
    for (at, _) in index.graph.files() {
        let Some(symbols) = index.graph.symbols(at) else {
            continue;
        };
        let mut heirs: Vec<&str> = symbols.overrides().map(|(heir, _)| heir).collect();
        heirs.sort_unstable();
        heirs.dedup();
        for heir in heirs {
            let Some((position, decl)) = symbols
                .declarations()
                .enumerate()
                .find(|(_, decl)| member_in(decl.name(), heir) == Some(member))
            else {
                continue;
            };
            if overridden(index, at, decl.name()).contains(&(file, place)) {
                found.insert((at, position));
            }
        }
    }
    found.into_iter().collect()
}

/// The file that declares `spelled`, a name written in `file`, and the name it declares it under. `lib.Base` through `import * as lib` is `Base` of the file `lib` is.
fn declaring<'a>(index: &'a Index, file: FileId, spelled: &'a str) -> Option<(FileId, &'a str)> {
    let mut seen = Seen::new();
    if let Some(found) = own(index, file, spelled, &mut seen) {
        return Some(found);
    }
    let (namespace, name) = spelled.split_once(MEMBER_SEPARATOR)?;
    for (taken, bound, resolved) in bindings(index, file) {
        if taken == "*" && bound == namespace {
            for &target in resolved.to.files() {
                if let Some(found) = exported(index, target, name, &mut seen) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// What `name` is in `file`: a declaration of it, or the name an import binds, followed to where the imported file declares it.
fn own<'a>(
    index: &'a Index,
    file: FileId,
    name: &'a str,
    seen: &mut Seen<'a>,
) -> Option<(FileId, &'a str)> {
    if !seen.insert((file, name, false)) {
        return None;
    }
    let declared = index.graph.symbols(file).and_then(|symbols| {
        symbols
            .declarations()
            .map(crate::graph::Declaration::name)
            .find(|&declared| declared == name)
    });
    if let Some(declared) = declared {
        return Some((file, declared));
    }
    for (taken, bound, resolved) in bindings(index, file) {
        if bound != name {
            continue;
        }
        for &target in resolved.to.files() {
            if let Some(found) = exported(index, target, taken, seen) {
                return Some(found);
            }
        }
    }
    None
}

/// What `file` exports as `name`: the local it exports it from, which may be an import of the file, what it declares, what it re-exports, and what an `export *` line takes in. `export *` never forwards `default`.
///
/// The TypeScript linker follows the same stars, aliases and defaults in `export_of` (`crates/lang-ts/src/link.rs`), as does `exported` in `exports.rs`. The copy stays, because the core knows no language, and the test `the_heads_of_a_class_and_the_mounts_are_found_through_the_same_stars_aliases_and_defaults` in `crates/lang-ts/tests/typescript/exports.rs` pins them against each other.
fn exported<'a>(
    index: &'a Index,
    file: FileId,
    name: &'a str,
    seen: &mut Seen<'a>,
) -> Option<(FileId, &'a str)> {
    if !seen.insert((file, name, true)) {
        return None;
    }
    let local = index.graph.symbols(file).and_then(|symbols| {
        symbols
            .exports()
            .find(|&(exported, _)| exported == name)
            .map(|(_, local)| local)
    });
    if let Some(found) = own(index, file, local.unwrap_or(name), seen) {
        return Some(found);
    }
    if name == "default" {
        return None;
    }
    star_targets(&index.graph, file).find_map(|target| exported(index, target, name, seen))
}

/// Each name an import of `file` takes, the local or exported name it binds it as, and the import. An import that does not say which names it binds, and one of a kind that binds none, give nothing.
fn bindings(
    index: &Index,
    file: FileId,
) -> impl Iterator<Item = (&str, &str, &ResolvedImport)> + '_ {
    index
        .records
        .get(file.index())
        .map_or(&[][..], |record| record.imports.as_slice())
        .iter()
        .filter(|resolved| {
            matches!(
                resolved.import.kind,
                ImportKind::Static | ImportKind::ReExport | ImportKind::Require
            ) && resolved.import.names.len() == resolved.import.exported.len()
        })
        .flat_map(|resolved| {
            let import = &resolved.import;
            import
                .names
                .iter()
                .zip(&import.exported)
                .map(move |(taken, bound)| (taken.as_str(), bound.as_str(), resolved))
        })
}
