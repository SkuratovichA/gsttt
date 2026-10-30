//! The declarations of each file and what each uses, stored flat: every string of every file back to back in one buffer, so that building them costs a copy, a snapshot holds one buffer and not one string per name, and reading a declaration is a few slices.
//!
//! Nothing is numbered or looked up while the graph is built. A walk that asks about a name compares strings in the few files it reaches.

use serde::{Deserialize, Serialize};

use crate::fact::{Line, Symbols};
use crate::ids::{FileId, Id};

use super::len_u32;

/// Marks a file without symbols in [`Table::slots`].
const NONE: u32 = u32::MAX;

/// The symbols of every file of a graph.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
pub(super) struct Table {
    /// Every string, back to back.
    text: String,
    /// `ends[n]` is where string `n` stops in `text`; it starts where string `n - 1` stopped.
    ends: Vec<u32>,
    decls: Vec<Decl>,
    /// The position in `spans` of the symbols of each file, [`NONE`] for a file whose connector records none.
    slots: Vec<u32>,
    spans: Vec<Span>,
}

/// One declaration: the string of its name is followed by the strings of what it uses.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct Decl {
    /// The number of the string of its name.
    name: u32,
    /// How many strings follow the name.
    uses: u32,
    line: Line,
    end: Line,
}

/// Where the symbols of one file are: a run of declarations, a run of strings for what it uses as it loads, and a run of strings in pairs for what it exports from a local of another name.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
struct Span {
    decls: u32,
    decl_count: u32,
    on_load: u32,
    on_load_count: u32,
    exports: u32,
    export_count: u32,
}

impl Table {
    /// A table for `files` files, none of which has symbols yet.
    pub(super) fn new(files: usize) -> Self {
        Self {
            slots: vec![NONE; files],
            ..Self::default()
        }
    }

    /// Copies the symbols of `file` into the table, `None` for a file whose connector records none. A file given twice reads as the second, and the first stays in the buffer unread. A file the table was not made for is left out.
    pub(super) fn insert(&mut self, file: FileId, symbols: Option<&Symbols>) {
        let index = file.index();
        if index >= self.slots.len() {
            return;
        }
        let slot = match symbols {
            Some(symbols) => {
                let span = self.copy(symbols);
                self.spans.push(span);
                len_u32(self.spans.len() - 1)
            }
            None => NONE,
        };
        if let Some(entry) = self.slots.get_mut(index) {
            *entry = slot;
        }
    }

    fn copy(&mut self, symbols: &Symbols) -> Span {
        let decls = len_u32(self.decls.len());
        for declared in &symbols.declared {
            let name = self.push(&declared.name);
            for used in &declared.uses {
                self.push(used);
            }
            self.decls.push(Decl {
                name,
                uses: len_u32(declared.uses.len()),
                line: declared.line,
                end: declared.end,
            });
        }
        let on_load = len_u32(self.ends.len());
        for name in &symbols.on_load {
            self.push(name);
        }
        let exports = len_u32(self.ends.len());
        for export in &symbols.exports {
            self.push(&export.name);
            self.push(&export.local);
        }
        Span {
            decls,
            decl_count: len_u32(symbols.declared.len()),
            on_load,
            on_load_count: len_u32(symbols.on_load.len()),
            exports,
            export_count: len_u32(symbols.exports.len()),
        }
    }

    /// Appends one string and gives its number.
    fn push(&mut self, text: &str) -> u32 {
        let number = len_u32(self.ends.len());
        self.text.push_str(text);
        self.ends.push(len_u32(self.text.len()));
        number
    }

    /// The string `number`; empty for a number the table does not hold.
    fn text(&self, number: u32) -> &str {
        let number = number as usize;
        let start = number
            .checked_sub(1)
            .and_then(|before| self.ends.get(before))
            .map_or(0, |&end| end as usize);
        let end = self.ends.get(number).map_or(start, |&end| end as usize);
        self.text.get(start..end).unwrap_or_default()
    }

    pub(super) fn of(&self, file: FileId) -> Option<FileSymbols<'_>> {
        let slot = *self.slots.get(file.index())?;
        let span = self.spans.get(slot as usize)?;
        Some(FileSymbols { table: self, span })
    }
}

/// The declarations of one file, what its code uses as it loads, and what it exports from a local of another name.
#[derive(Clone, Copy, Debug)]
pub struct FileSymbols<'g> {
    table: &'g Table,
    span: &'g Span,
}

impl<'g> FileSymbols<'g> {
    /// How many declarations the file has.
    pub fn len(self) -> usize {
        self.span.decl_count as usize
    }

    /// Whether the file has no declarations.
    pub fn is_empty(self) -> bool {
        self.span.decl_count == 0
    }

    /// The declaration at `index` among those of [`FileSymbols::declarations`].
    pub fn declaration(self, index: usize) -> Option<Declaration<'g>> {
        let position = (self.span.decls as usize).checked_add(index)?;
        let decl = self
            .table
            .decls
            .get(position)
            .filter(|_| index < self.len())?;
        Some(Declaration {
            table: self.table,
            decl,
        })
    }

    /// The declarations by the line they start on and then by name, as the connector recorded them.
    pub fn declarations(self) -> impl ExactSizeIterator<Item = Declaration<'g>> + 'g {
        let start = self.span.decls as usize;
        let table = self.table;
        table
            .decls
            .get(start..start.saturating_add(self.len()))
            .unwrap_or_default()
            .iter()
            .map(move |decl| Declaration { table, decl })
    }

    /// What the code of the file refers to as the file loads.
    pub fn on_load(self) -> impl ExactSizeIterator<Item = &'g str> + 'g {
        let table = self.table;
        let first = self.span.on_load;
        (0..self.span.on_load_count).map(move |offset| table.text(first.saturating_add(offset)))
    }

    /// Each name the file exports from a local of another name, as the name it is exported as and the local behind it.
    pub fn exports(self) -> impl ExactSizeIterator<Item = (&'g str, &'g str)> + 'g {
        let table = self.table;
        let first = self.span.exports;
        (0..self.span.export_count).map(move |offset| {
            let name = first.saturating_add(offset.saturating_mul(2));
            (table.text(name), table.text(name.saturating_add(1)))
        })
    }
}

/// One declaration of a file.
#[derive(Clone, Copy, Debug)]
pub struct Declaration<'g> {
    table: &'g Table,
    decl: &'g Decl,
}

impl<'g> Declaration<'g> {
    /// `getHash`, `ModuleRunner.import` for a member of a class, `default` for `export default` of an expression.
    pub fn name(self) -> &'g str {
        self.table.text(self.decl.name)
    }

    /// The line it starts on, from 1.
    pub fn line(self) -> Line {
        self.decl.line
    }

    /// The line it ends on, from 1.
    pub fn end(self) -> Line {
        self.decl.end
    }

    /// The names it refers to, in the order the connector recorded them, which is sorted.
    pub fn uses(self) -> impl ExactSizeIterator<Item = &'g str> + 'g {
        let table = self.table;
        let first = self.decl.name.saturating_add(1);
        (0..self.decl.uses).map(move |offset| table.text(first.saturating_add(offset)))
    }
}

#[cfg(test)]
mod tests {
    use crate::fact::{Declared, LocalExport};
    use crate::testing::owned;

    use super::*;

    fn declared(name: &str, line: Line, end: Line, uses: &[&str]) -> Declared {
        Declared {
            name: name.to_owned(),
            line,
            end,
            uses: owned(uses),
        }
    }

    fn symbols() -> Symbols {
        Symbols {
            declared: vec![
                declared("run", 3, 5, &["foo", "stop"]),
                declared("stop", 7, 9, &[]),
            ],
            on_load: owned(&["setup"]),
            exports: vec![LocalExport {
                name: "default".to_owned(),
                local: "run".to_owned(),
            }],
        }
    }

    fn file(index: u32) -> FileId {
        FileId::from_raw(index)
    }

    #[test]
    fn a_file_reads_back_what_it_was_given() {
        let mut table = Table::new(2);
        table.insert(file(1), Some(&symbols()));

        let read = table.of(file(1)).unwrap();
        let rows: Vec<(&str, Line, Line, Vec<&str>)> = read
            .declarations()
            .map(|declaration| {
                (
                    declaration.name(),
                    declaration.line(),
                    declaration.end(),
                    declaration.uses().collect(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("run", 3, 5, vec!["foo", "stop"]),
                ("stop", 7, 9, Vec::new())
            ]
        );
        assert_eq!(read.on_load().collect::<Vec<_>>(), ["setup"]);
        assert_eq!(read.exports().collect::<Vec<_>>(), [("default", "run")]);
        assert_eq!(read.len(), 2);
        assert_eq!(read.declaration(1).map(Declaration::name), Some("stop"));
        assert!(read.declaration(2).is_none());
    }

    #[test]
    fn a_file_without_symbols_is_not_a_file_with_none() {
        let mut table = Table::new(3);
        table.insert(file(0), None);
        table.insert(file(1), Some(&Symbols::default()));

        assert!(table.of(file(0)).is_none());
        assert!(table.of(file(1)).is_some_and(FileSymbols::is_empty));
        assert!(table.of(file(2)).is_none());
        assert!(table.of(file(9)).is_none());
    }

    #[test]
    fn files_given_in_any_order_keep_their_own_declarations() {
        let mut table = Table::new(3);
        let other = Symbols {
            declared: vec![declared("other", 1, 1, &["run"])],
            ..Symbols::default()
        };
        table.insert(file(2), Some(&symbols()));
        table.insert(file(0), Some(&other));

        let names = |id: u32| -> Vec<&str> {
            table
                .of(file(id))
                .unwrap()
                .declarations()
                .map(Declaration::name)
                .collect()
        };
        assert_eq!(names(0), ["other"]);
        assert_eq!(names(2), ["run", "stop"]);
    }

    #[test]
    fn a_file_given_twice_reads_as_the_second_time() {
        let mut table = Table::new(1);
        table.insert(file(0), Some(&symbols()));
        table.insert(file(0), None);
        assert!(table.of(file(0)).is_none());
        table.insert(file(0), Some(&Symbols::default()));
        assert!(table.of(file(0)).is_some_and(FileSymbols::is_empty));
    }

    #[test]
    fn a_file_the_table_was_not_made_for_is_left_out() {
        let mut table = Table::new(1);
        table.insert(file(5), Some(&symbols()));
        assert_eq!(table, Table::new(1));
    }
}
