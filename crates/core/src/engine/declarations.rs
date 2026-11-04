//! Which declarations of a file differ between two versions of it, so that `--changed` can walk from them in place of the whole file.
//!
//! A version is compared by lines, not by syntax: the text of each declaration is the lines it owns, with comments and import statements blanked, and a declaration differs when the lines it owns differ. A removed or renamed declaration is listed by its old name; anything else the lines cannot tell by declaration, such as a moved declaration or a changed top-level statement, makes the whole file changed.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use xxhash_rust::xxh3::{Xxh3, xxh3_64};

use crate::fact::{
    FileFacts, Import, ImportFlags, ImportKind, Line, MEMBER_SEPARATOR, ParseStatus, Symbols, Use,
    belongs_to,
};

/// One version of a file: its facts, and its text with comments and import statements blanked line for line.
#[derive(Clone, Copy)]
pub(super) struct Version<'a> {
    pub facts: &'a FileFacts,
    pub code: &'a str,
}

/// How two versions of a file differ by declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Declarations {
    /// The declarations that differ and exist in the new version, sorted, each once.
    pub changed: Vec<String>,
    /// The names the old version declares or exports and the new one does not, sorted, each once. The new text no longer says who imported them, but an importer that was not changed still names them.
    pub removed: Vec<String>,
}

/// The declarations that differ, `None` when the difference cannot be told by declaration, and the file is then changed whole.
pub(super) fn changed_declarations(old: Version<'_>, new: Version<'_>) -> Option<Declarations> {
    let (old_symbols, new_symbols) = (symbols_of(old.facts)?, symbols_of(new.facts)?);
    let old_digests = Digests::of(old_symbols, old.code);
    let new_digests = Digests::of(new_symbols, new.code);
    // Only the declarations both versions hold take part in the order: a removed or a new one leaves the order of the others as it was, while a move, or a statement that changes, does not.
    if !shared(&old_digests, &new_digests).eq(shared(&new_digests, &old_digests)) {
        return None;
    }
    let mut changed: BTreeSet<&str> = new_digests
        .by_name
        .iter()
        .filter(|&(name, digest)| old_digests.by_name.get(name) != Some(digest))
        .map(|(&name, _)| name)
        .collect();
    if blanked_others(old.facts) != blanked_others(new.facts) {
        return None;
    }
    let locals = changed_locals(old.facts, new.facts);
    if !locals.is_empty() {
        let uses_local = |used: &Use| locals.iter().any(|local| belongs_to(&used.name, local));
        if new_symbols.on_load.iter().any(uses_local) {
            return None;
        }
        changed.extend(
            new_symbols
                .declared
                .iter()
                .filter(|decl| decl.uses.iter().any(uses_local))
                .map(|decl| decl.name.as_str()),
        );
    }
    Some(Declarations {
        changed: changed.into_iter().map(str::to_owned).collect(),
        removed: removed(old.facts, new.facts, &old_digests, &new_digests),
    })
}

/// The names the old version declares, members included, or exports, that the new one does not, sorted, once each. A declaration that stops being exported is among them, since its importers lose it.
fn removed(
    old: &FileFacts,
    new: &FileFacts,
    old_digests: &Digests<'_>,
    new_digests: &Digests<'_>,
) -> Vec<String> {
    let mut gone: BTreeSet<&str> = old_digests
        .by_name
        .keys()
        .filter(|name| !new_digests.by_name.contains_key(*name))
        .copied()
        .collect();
    if let (Some(before), Some(after)) = (&old.exports, &new.exports) {
        gone.extend(
            before
                .iter()
                .filter(|name| !after.contains(name))
                .map(String::as_str),
        );
    }
    gone.into_iter().map(str::to_owned).collect()
}

/// The steps of the top level of `digests` that the `other` version holds too: every statement, and the declarations of that version.
fn shared<'d, 'a>(
    digests: &'d Digests<'a>,
    other: &'d Digests<'_>,
) -> impl Iterator<Item = &'d Item<'a>> {
    digests.order.iter().filter(|item| match item {
        Item::Decl(name) => other.by_name.contains_key(name),
        Item::Rest(_) => true,
    })
}

/// The declarations of a file that was read in full, `None` for a file read past errors, a failed one, or one whose connector records no declarations.
fn symbols_of(facts: &FileFacts) -> Option<&Symbols> {
    if facts.status != ParseStatus::Ok {
        return None;
    }
    facts.symbols.as_ref()
}

/// One step of the top level of a version, in line order.
#[derive(PartialEq, Eq)]
enum Item<'a> {
    /// A top-level declaration starts.
    Decl(&'a str),
    /// The hash of a line that no declaration owns.
    Rest(u64),
}

/// What each declaration of a version holds, and the order of the top level, which says where each declaration stands among the statements that no declaration owns.
struct Digests<'a> {
    by_name: BTreeMap<&'a str, u64>,
    order: Vec<Item<'a>>,
}

impl<'a> Digests<'a> {
    /// Hands every non-blank line of `code` to the declarations that own it. A line belongs to the innermost declaration open on it and to every declaration it starts or ends, so `class A extends B { m() {}` belongs to `A` and to `A.m` while a line in the middle of `A.m` belongs to `A.m` alone. A line no declaration is open on belongs to the rest. Each line is hashed once, so a minified file with thousands of declarations on one line costs no more than any other.
    fn of(symbols: &'a Symbols, code: &str) -> Self {
        let mut declared: Vec<_> = symbols.declared.iter().collect();
        declared.sort_by_key(|decl| (decl.line, Reverse(decl.end.max(decl.line))));
        // Declarations over the same lines are one group.
        let mut spans: Vec<(Line, Line)> = Vec::new();
        let mut names: Vec<(&'a str, usize)> = Vec::with_capacity(declared.len());
        for decl in declared {
            let span = (decl.line, decl.end.max(decl.line));
            if spans.last() != Some(&span) {
                spans.push(span);
            }
            names.push((decl.name.as_str(), spans.len().saturating_sub(1)));
        }
        let mut by_end: Vec<usize> = (0..spans.len()).collect();
        by_end.sort_by_key(|&group| spans.get(group).map(|span| span.1));

        let mut owned: Vec<Vec<u64>> = vec![Vec::new(); spans.len()];
        let mut order = Vec::new();
        let mut open: Vec<usize> = Vec::new();
        let mut next = 0;
        let mut announced = 0;
        let mut ending = 0;
        let mut touched: Vec<usize> = Vec::new();
        for (index, text) in code.split('\n').enumerate() {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let number = Line::try_from(index).map_or(Line::MAX, |index| index.saturating_add(1));
            let hash = xxh3_64(text.as_bytes());
            let end_of = |group: usize| spans.get(group).map_or(0, |span| span.1);
            touched.clear();
            while open.last().is_some_and(|&top| end_of(top) < number) {
                open.pop();
            }
            while let Some(&(first, _)) = spans.get(next)
                && first <= number
            {
                if first == number {
                    touched.push(next);
                }
                // The names are in the order of their groups, so one cursor visits each once.
                while let Some(&(name, group)) = names.get(announced)
                    && group == next
                {
                    if !name.contains(MEMBER_SEPARATOR) {
                        order.push(Item::Decl(name));
                    }
                    announced += 1;
                }
                open.push(next);
                next += 1;
            }
            while open.last().is_some_and(|&top| end_of(top) < number) {
                open.pop();
            }
            let Some(&innermost) = open.last() else {
                order.push(Item::Rest(hash));
                continue;
            };
            touched.push(innermost);
            while by_end
                .get(ending)
                .is_some_and(|&group| end_of(group) < number)
            {
                ending += 1;
            }
            touched.extend(
                by_end
                    .iter()
                    .skip(ending)
                    .take_while(|&&group| end_of(group) == number),
            );
            touched.sort_unstable();
            touched.dedup();
            for &group in &touched {
                if let Some(lines) = owned.get_mut(group) {
                    lines.push(hash);
                }
            }
        }
        let digests: Vec<u64> = owned.iter().map(|lines| digest(lines)).collect();
        Self {
            by_name: names
                .into_iter()
                .filter_map(|(name, group)| Some((name, *digests.get(group)?)))
                .collect(),
            order,
        }
    }
}

/// The hash of a sequence of line hashes.
fn digest(lines: &[u64]) -> u64 {
    let mut hasher = Xxh3::new();
    for line in lines {
        hasher.update(&line.to_le_bytes());
    }
    hasher.digest()
}

/// Whether the import is one whose names are read from the facts: static, with the local name recorded for each name it takes.
fn is_bound(import: &Import) -> bool {
    import.kind == ImportKind::Static
        && !import.names.is_empty()
        && import.exported.len() == import.names.len()
}

/// The imports that are blanked from the text and not read as bindings, by kind and specifier in order, so that a duplicated one counts.
fn blanked_others(facts: &FileFacts) -> Vec<(u16, &str)> {
    let mut others: Vec<(u16, &str)> = facts
        .imports
        .iter()
        .filter(|import| match import.kind {
            ImportKind::SideEffect | ImportKind::Reference => true,
            ImportKind::Static => !is_bound(import),
            _ => false,
        })
        .map(|import| (import.kind.bit(), import.spec.as_str()))
        .collect();
    others.sort_unstable();
    others
}

/// What the static imports bind: the specifier, the name taken, the local name and whether the import is of types only, as one that gains or loses `type` starts to load a module or stops.
fn bindings(facts: &FileFacts) -> BTreeSet<(&str, &str, &str, bool)> {
    facts
        .imports
        .iter()
        .filter(|import| is_bound(import))
        .flat_map(|import| {
            import
                .names
                .iter()
                .zip(&import.exported)
                .map(|(taken, local)| {
                    (
                        import.spec.as_str(),
                        taken.as_str(),
                        local.as_str(),
                        import.flags.contains(ImportFlags::TYPE_ONLY),
                    )
                })
        })
        .collect()
}

/// The local names that one version binds differently from the other.
fn changed_locals<'a>(old: &'a FileFacts, new: &'a FileFacts) -> BTreeSet<&'a str> {
    let (old, new) = (bindings(old), bindings(new));
    old.symmetric_difference(&new)
        .map(|&(_, _, local, _)| local)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::fact::Declared;

    fn declared(name: &str, line: Line, end: Line, uses: &[&str]) -> Declared {
        Declared {
            name: name.to_owned(),
            line,
            end,
            uses: uses
                .iter()
                .map(|used| Use {
                    name: (*used).to_owned(),
                    line,
                })
                .collect(),
        }
    }

    fn facts(declared: Vec<Declared>) -> FileFacts {
        FileFacts {
            symbols: Some(Symbols {
                declared,
                ..Symbols::default()
            }),
            ..FileFacts::default()
        }
    }

    fn import(spec: &str, names: &[(&str, &str)]) -> Import {
        let mut import = Import::new(spec, ImportKind::Static, 1);
        import.names = names.iter().map(|(taken, _)| (*taken).to_owned()).collect();
        import.exported = names.iter().map(|(_, local)| (*local).to_owned()).collect();
        import
    }

    fn compare(old: (&FileFacts, &str), new: (&FileFacts, &str)) -> Option<Vec<String>> {
        differ(old, new).map(|declarations| declarations.changed)
    }

    fn differ(old: (&FileFacts, &str), new: (&FileFacts, &str)) -> Option<Declarations> {
        changed_declarations(
            Version {
                facts: old.0,
                code: old.1,
            },
            Version {
                facts: new.0,
                code: new.1,
            },
        )
    }

    /// Two functions and a class with two methods, one of which shares its first line with the class.
    fn layout() -> FileFacts {
        facts(vec![
            declared("a", 1, 3, &[]),
            declared("b", 5, 5, &["a"]),
            declared("C", 7, 14, &[]),
            declared("C.m", 7, 9, &[]),
            declared("C.n", 11, 13, &[]),
        ])
    }

    const TEXT: &str = "function a() {\n  return 1;\n}\n\nconst b = a();\n\nclass C { m() {\n  return 2;\n}\n\n  n() {\n    return 3;\n  }\n}\n";

    #[test]
    fn a_text_that_did_not_change_changes_nothing() {
        let facts = layout();
        assert_eq!(compare((&facts, TEXT), (&facts, TEXT)), Some(Vec::new()));
    }

    #[test]
    fn reindenting_changes_nothing() {
        let facts = layout();
        let spaced = TEXT.replace("  return", "      return");
        assert_eq!(compare((&facts, TEXT), (&facts, &spaced)), Some(Vec::new()));
    }

    #[test]
    fn blank_lines_that_move_declarations_change_nothing() {
        let old = layout();
        // A blank line above `b` moves `b` and the class one line down.
        let spaced = TEXT.replace("\nconst b", "\n\nconst b");
        let new = facts(vec![
            declared("a", 1, 3, &[]),
            declared("b", 6, 6, &["a"]),
            declared("C", 8, 15, &[]),
            declared("C.m", 8, 10, &[]),
            declared("C.n", 12, 14, &[]),
        ]);
        assert_eq!(compare((&old, TEXT), (&new, &spaced)), Some(Vec::new()));
    }

    #[test]
    fn a_line_in_the_middle_of_a_member_changes_that_member_alone() {
        let facts = layout();
        let edited = TEXT.replace("return 3", "return 4");
        assert_eq!(
            compare((&facts, TEXT), (&facts, &edited)),
            Some(vec!["C.n".to_owned()])
        );
    }

    #[test]
    fn a_header_on_the_line_of_a_nested_one_liner_changes_both() {
        let facts = layout();
        let edited = TEXT.replace("class C {", "class C extends B {");
        assert_eq!(
            compare((&facts, TEXT), (&facts, &edited)),
            Some(vec!["C".to_owned(), "C.m".to_owned()])
        );
    }

    #[test]
    fn a_line_of_a_class_outside_its_members_changes_the_class_alone() {
        let facts = facts(vec![declared("C", 1, 7, &[]), declared("C.m", 2, 4, &[])]);
        let old = "class C {\n  m() {\n    x;\n  }\n  field = 1;\n\n}\n";
        let new = old.replace("field = 1", "field = 2");
        assert_eq!(
            compare((&facts, old), (&facts, &new)),
            Some(vec!["C".to_owned()])
        );
    }

    #[test]
    fn declarations_over_the_same_lines_change_together() {
        let facts = facts(vec![declared("x", 1, 1, &[]), declared("y", 1, 1, &[])]);
        assert_eq!(
            compare(
                (&facts, "const { x, y } = f(1);\n"),
                (&facts, "const { x, y } = f(2);\n")
            ),
            Some(vec!["x".to_owned(), "y".to_owned()])
        );
    }

    #[test]
    fn a_one_line_file_is_its_declaration() {
        let facts = facts(vec![declared("f", 1, 1, &[])]);
        assert_eq!(
            compare(
                (&facts, "export const f = 1;"),
                (&facts, "export const f = 2;")
            ),
            Some(vec!["f".to_owned()])
        );
    }

    #[test]
    fn moving_a_declaration_changes_the_file_whole() {
        let old = facts(vec![declared("a", 1, 1, &[]), declared("b", 2, 2, &[])]);
        let new = facts(vec![declared("b", 1, 1, &[]), declared("a", 2, 2, &[])]);
        assert_eq!(
            compare(
                (&old, "const a = 1;\nconst b = 2;\n"),
                (&new, "const b = 2;\nconst a = 1;\n")
            ),
            None
        );
    }

    #[test]
    fn a_declaration_added_between_two_others_is_the_only_one_changed() {
        let old = facts(vec![declared("a", 1, 1, &[]), declared("c", 2, 2, &[])]);
        let new = facts(vec![
            declared("a", 1, 1, &[]),
            declared("b", 2, 2, &[]),
            declared("c", 3, 3, &[]),
        ]);
        assert_eq!(
            compare(
                (&old, "const a = 1;\nconst c = 3;\n"),
                (&new, "const a = 1;\nconst b = 2;\nconst c = 3;\n")
            ),
            Some(vec!["b".to_owned()])
        );
    }

    #[test]
    fn a_statement_moved_across_a_declaration_changes_the_file_whole() {
        let old = facts(vec![declared("a", 2, 2, &[])]);
        let new = facts(vec![declared("a", 1, 1, &[])]);
        assert_eq!(
            compare(
                (&old, "run();\nconst a = 1;\n"),
                (&new, "const a = 1;\nrun();\n")
            ),
            None
        );
    }

    #[test]
    fn a_version_not_read_in_full_gives_no_declarations() {
        let old = facts(vec![declared("a", 1, 1, &[])]);
        let mut partial = old.clone();
        partial.status = ParseStatus::Recovered {
            errors: 1,
            first_error_line: 1,
        };
        let text = "const a = 1;\n";
        assert_eq!(compare((&old, text), (&partial, text)), None);
        assert_eq!(compare((&partial, text), (&old, text)), None);
    }

    #[test]
    fn an_import_that_stops_being_type_only_changes_the_declarations_that_use_it() {
        let mut typed = import("./t", &[("T", "T")]);
        typed.flags = ImportFlags::TYPE_ONLY;
        let loaded = import("./t", &[("T", "T")]);
        let mut old = facts(vec![declared("a", 1, 1, &["T"])]);
        old.imports.push(typed);
        let mut new = facts(vec![declared("a", 1, 1, &["T"])]);
        new.imports.push(loaded);
        let text = "const a = f(T);\n";
        assert_eq!(
            compare((&old, text), (&new, text)),
            Some(vec!["a".to_owned()])
        );
    }

    #[test]
    fn a_new_declaration_is_changed() {
        let old = facts(vec![declared("a", 1, 1, &[])]);
        let new = facts(vec![declared("a", 1, 1, &[]), declared("b", 2, 2, &[])]);
        assert_eq!(
            compare(
                (&old, "const a = 1;\n"),
                (&new, "const a = 1;\nconst b = 2;\n")
            ),
            Some(vec!["b".to_owned()])
        );
    }

    fn gone(changed: &[&str], removed: &[&str]) -> Declarations {
        Declarations {
            changed: changed.iter().map(|name| (*name).to_owned()).collect(),
            removed: removed.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn a_removed_or_renamed_declaration_is_listed_as_removed() {
        let old = facts(vec![declared("a", 1, 1, &[]), declared("b", 2, 2, &[])]);
        let removed = facts(vec![declared("a", 1, 1, &[])]);
        let renamed = facts(vec![declared("a", 1, 1, &[]), declared("c", 2, 2, &[])]);
        let text = "const a = 1;\nconst b = 2;\n";
        assert_eq!(
            differ((&old, text), (&removed, "const a = 1;\n")),
            Some(gone(&[], &["b"]))
        );
        assert_eq!(
            differ((&old, text), (&renamed, "const a = 1;\nconst c = 2;\n")),
            Some(gone(&["c"], &["b"]))
        );
    }

    #[test]
    fn a_removed_member_is_listed_with_its_class() {
        let old = facts(vec![
            declared("C", 1, 5, &[]),
            declared("C.m", 2, 2, &[]),
            declared("C.n", 3, 3, &[]),
        ]);
        let new = facts(vec![declared("C", 1, 4, &[]), declared("C.n", 2, 2, &[])]);
        let result = differ(
            (&old, "class C {\n m() {}\n n() {}\n}\n"),
            (&new, "class C {\n n() {}\n}\n"),
        );
        assert_eq!(
            result.map(|found| found.removed),
            Some(vec!["C.m".to_owned()])
        );
    }

    #[test]
    fn a_declaration_that_stops_being_exported_is_removed() {
        let mut old = facts(vec![declared("a", 1, 1, &[])]);
        old.exports = Some(vec!["a".to_owned(), "default".to_owned()]);
        let mut new = facts(vec![declared("a", 1, 1, &[])]);
        new.exports = Some(vec!["a".to_owned()]);
        let text = "const a = 1;\n";
        assert_eq!(
            differ((&old, text), (&new, text)),
            Some(gone(&[], &["default"]))
        );
    }

    #[test]
    fn a_moved_declaration_next_to_a_removed_one_changes_the_file_whole() {
        let old = facts(vec![
            declared("a", 1, 1, &[]),
            declared("b", 2, 2, &[]),
            declared("c", 3, 3, &[]),
        ]);
        let new = facts(vec![declared("c", 1, 1, &[]), declared("a", 2, 2, &[])]);
        assert_eq!(
            differ(
                (&old, "const a = 1;\nconst b = 2;\nconst c = 3;\n"),
                (&new, "const c = 3;\nconst a = 1;\n")
            ),
            None
        );
    }

    #[test]
    fn a_changed_top_level_statement_changes_the_file_whole() {
        let facts = facts(vec![declared("a", 2, 2, &[])]);
        assert_eq!(
            compare(
                (&facts, "setup(1);\nconst a = 1;\n"),
                (&facts, "setup(2);\nconst a = 1;\n")
            ),
            None
        );
    }

    #[test]
    fn a_file_not_read_in_full_is_changed_whole() {
        let whole = layout();
        let recovered = FileFacts {
            status: ParseStatus::Recovered {
                errors: 1,
                first_error_line: 1,
            },
            ..layout()
        };
        let bare = FileFacts::default();
        assert_eq!(compare((&whole, TEXT), (&recovered, TEXT)), None);
        assert_eq!(compare((&bare, TEXT), (&whole, TEXT)), None);
    }

    #[test]
    fn a_new_import_nobody_uses_changes_nothing() {
        let old = facts(vec![declared("a", 1, 1, &[])]);
        let mut new = facts(vec![declared("a", 1, 1, &[])]);
        new.imports.push(import("./x", &[("x", "x")]));
        assert_eq!(
            compare((&old, "const a = 1;\n"), (&new, "const a = 1;\n")),
            Some(Vec::new())
        );
    }

    #[test]
    fn a_re_pointed_import_changes_the_declarations_that_use_it() {
        let mut old = facts(vec![
            declared("a", 1, 1, &["clock.now"]),
            declared("b", 2, 2, &[]),
        ]);
        old.imports.push(import("./a", &[("clock", "clock")]));
        let mut new = old.clone();
        new.imports = vec![import("./b", &[("clock", "clock")])];
        let text = "const a = clock.now();\nconst b = 1;\n";
        assert_eq!(
            compare((&old, text), (&new, text)),
            Some(vec!["a".to_owned()])
        );
    }

    #[test]
    fn a_re_pointed_import_used_as_the_file_loads_changes_the_file_whole() {
        let mut old = facts(vec![declared("a", 2, 2, &[])]);
        old.symbols.as_mut().unwrap().on_load = vec![Use {
            name: "clock".to_owned(),
            line: 1,
        }];
        old.imports.push(import("./a", &[("clock", "clock")]));
        let mut new = old.clone();
        new.imports = vec![import("./b", &[("clock", "clock")])];
        let text = "start(clock);\nconst a = 1;\n";
        assert_eq!(compare((&old, text), (&new, text)), None);
    }

    #[test]
    fn a_side_effect_import_or_one_without_bindings_changes_the_file_whole() {
        let old = facts(vec![declared("a", 1, 1, &[])]);
        let mut side = old.clone();
        side.imports
            .push(Import::new("./polyfill", ImportKind::SideEffect, 1));
        let mut bare = old.clone();
        bare.imports.push(Import::new("./x", ImportKind::Static, 1));
        let text = "const a = 1;\n";
        assert_eq!(compare((&old, text), (&side, text)), None);
        assert_eq!(compare((&old, text), (&bare, text)), None);
    }

    #[test]
    fn thousands_of_declarations_on_one_line_are_not_a_quadratic_pass() {
        let count = 20_000_u32;
        let declarations = |_: u32| declared("d", 1, 1, &[]);
        let many: Vec<Declared> = (0..count)
            .map(|index| Declared {
                name: format!("d{index}"),
                ..declarations(index)
            })
            .collect();
        let facts = facts(many);
        let text = "x".repeat(200_000);
        let edited = text.replacen('x', "y", 1);
        let started = Instant::now();
        let changed = compare((&facts, &text), (&facts, &edited)).unwrap();
        assert_eq!(changed.len(), 20_000);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn thousands_of_nested_lines_are_not_a_quadratic_pass() {
        let count = 20_000_u32;
        let declared: Vec<Declared> = (0..count)
            .map(|index| Declared {
                name: format!("d{index}"),
                line: index + 1,
                end: count * 2 - index,
                uses: Vec::new(),
            })
            .collect();
        let facts = facts(declared);
        let text = "x\n".repeat(usize::try_from(count * 2).unwrap());
        let started = Instant::now();
        assert_eq!(compare((&facts, &text), (&facts, &text)), Some(Vec::new()));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
