//! The nesting limit of a TOML file, counted on the events of the parser before any value is built from it.
//!
//! Deserializing a nested value recurses, in serde and again in the `Drop` of the value. `toml` bounds part of that, with limits that add up: inline tables and arrays nested deeper than 80 are refused, and so are keys of 80 parts or more, but each part of a header that names an array of tables adds a level of its own, so a file within every one of them can still nest more than two hundred deep, which a small stack does not hold. The check here counts all of them together and returns `false` from `array_open` and `inline_table_open` once the limit is passed, which makes the parser skip that value without descending into it, so that the check itself never goes deeper than the limit.

use std::collections::BTreeSet;
use std::ops::Range;

use toml_parser::decoder::Encoding;
use toml_parser::parser::{Event, EventKind, EventReceiver, parse_document};
use toml_parser::{ErrorSink, Source, Span};

/// How deep tables and arrays may nest in a config or a pack file, counted from the root table, which is not a level.
pub(crate) const MAX_DEPTH: u32 = 64;

/// Where the nesting of `text` first passes [`MAX_DEPTH`], as a byte range, or `None` when it stays within it. A syntax error is not reported here: the parse that follows reports it with the position and the words of its own, from the same events.
pub(crate) fn first_past_the_limit(text: &str) -> Option<Range<usize>> {
    let source = Source::new(text);
    let tokens = source.lex().into_vec();
    let mut check = DepthCheck {
        source,
        header: None,
        parts: Vec::new(),
        key_parts: 0,
        header_depth: 0,
        open: Vec::new(),
        pending: 0,
        arrays_of_tables: BTreeSet::new(),
        passed: None,
    };
    parse_document(&tokens, &mut check, &mut ());
    check.passed
}

/// What a header names: a table, or an element of an array of tables.
#[derive(Clone, Copy)]
enum Header {
    Table,
    ArrayOfTables,
}

struct DepthCheck<'s> {
    source: Source<'s>,
    /// The header being read, `None` between headers.
    header: Option<Header>,
    /// The decoded parts of the header being read, each with its span.
    parts: Vec<(String, Span)>,
    /// The simple keys of the key being read, which is more than one for a dotted key.
    key_parts: u32,
    /// The depth of the table the last header names, which is where the keys of the lines below it are.
    header_depth: u32,
    /// The depth of each inline array or table that is open.
    open: Vec<u32>,
    /// The dotted parts of the last key but its own, which the value that follows adds to its depth.
    pending: u32,
    /// The paths of the `[[…]]` headers seen so far, because a later header that goes through one of them counts it twice.
    arrays_of_tables: BTreeSet<Vec<String>>,
    /// Where the limit was first passed.
    passed: Option<Range<usize>>,
}

impl DepthCheck<'_> {
    /// The depth of what the keys being read sit in: the innermost open inline value, or else the table of the header.
    fn base(&self) -> u32 {
        self.open.last().copied().unwrap_or(self.header_depth)
    }

    fn pass(&mut self, span: Span) {
        if self.passed.is_none() {
            self.passed = Some(span.start()..span.end());
        }
    }

    fn open_header(&mut self, header: Header) {
        self.header = Some(header);
        self.parts.clear();
        self.open.clear();
        self.key_parts = 0;
        self.pending = 0;
    }

    /// The depth of the table a header names: one level per part, and one more for each part that is an array of tables, which holds a table per element.
    fn close_header(&mut self) {
        let Some(header) = self.header.take() else {
            return;
        };
        // The parts of the header are not the dotted parts of the key below it.
        self.key_parts = 0;
        let last = self.parts.len().saturating_sub(1);
        let mut path = Vec::with_capacity(self.parts.len().min(MAX_DEPTH as usize + 1));
        let mut depth = 0_u32;
        for (index, (name, span)) in self.parts.iter().enumerate() {
            path.push(name.clone());
            let names_array = matches!(header, Header::ArrayOfTables) && index == last;
            depth += 1 + u32::from(names_array || self.arrays_of_tables.contains(path.as_slice()));
            if depth > MAX_DEPTH {
                // Nothing below this header is read, so the loop stops here, which keeps a header of a hundred thousand parts from costing more than the limit allows.
                self.header_depth = depth;
                let span = *span;
                self.pass(span);
                return;
            }
        }
        self.header_depth = depth;
        if matches!(header, Header::ArrayOfTables) {
            self.arrays_of_tables.insert(path);
        }
    }

    /// An inline array or table opens one deeper than what holds it, plus the dotted parts of its key. Whether the parser may go into it.
    fn enter(&mut self, span: Span) -> bool {
        let depth = self.base().saturating_add(self.pending).saturating_add(1);
        self.pending = 0;
        // Pushed even when refused: the parser reports the close of a value it skipped.
        self.open.push(depth);
        if depth > MAX_DEPTH {
            self.pass(span);
            return false;
        }
        true
    }

    fn decode(&self, span: Span, encoding: Option<Encoding>) -> String {
        let mut part = String::new();
        if let Some(raw) =
            self.source
                .get(Event::new_unchecked(EventKind::SimpleKey, encoding, span))
        {
            raw.decode_key(&mut part, &mut ());
        }
        part
    }
}

impl EventReceiver for DepthCheck<'_> {
    fn std_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.open_header(Header::Table);
    }

    fn std_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.close_header();
    }

    fn array_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.open_header(Header::ArrayOfTables);
    }

    fn array_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.close_header();
    }

    fn inline_table_open(&mut self, span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.enter(span)
    }

    fn inline_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.open.pop();
    }

    fn array_open(&mut self, span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.enter(span)
    }

    fn array_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.open.pop();
    }

    fn simple_key(&mut self, span: Span, encoding: Option<Encoding>, _error: &mut dyn ErrorSink) {
        self.key_parts = self.key_parts.saturating_add(1);
        if self.header.is_some() {
            let name = self.decode(span, encoding);
            self.parts.push((name, span));
        }
    }

    fn key_val_sep(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.pending = self.key_parts.saturating_sub(1);
        self.key_parts = 0;
    }

    /// A scalar nests nothing, but the tables of its dotted key do: `a.b.c.d = 1` is three tables deep, and a thousand parts would be as many.
    fn scalar(&mut self, span: Span, _encoding: Option<Encoding>, _error: &mut dyn ErrorSink) {
        if self.base().saturating_add(self.pending) > MAX_DEPTH {
            self.pass(span);
        }
        self.pending = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passes_at(text: &str) -> Option<&str> {
        first_past_the_limit(text).and_then(|range| text.get(range))
    }

    #[test]
    fn a_file_within_the_limit_is_not_flagged() {
        let within = format!("[ts]\nx = {}1{}\n", "[".repeat(63), "]".repeat(63));
        assert_eq!(passes_at(&within), None);
        assert_eq!(passes_at(""), None);
        assert_eq!(passes_at("a = 1\n[b]\nc = { d = [1, 2] }\n"), None);
    }

    #[test]
    fn the_first_array_past_the_limit_is_where_it_is_flagged() {
        let past = format!("[ts]\nx = {}1{}\n", "[".repeat(64), "]".repeat(64));
        let range = first_past_the_limit(&past).unwrap();
        // The header is one level, so the 64th array is the 65th level.
        assert_eq!(range.start, "[ts]\nx = ".len() + 63);
        assert_eq!(passes_at(&past), Some("["));
    }

    #[test]
    fn dotted_keys_add_their_parts_but_the_last() {
        let key = |parts: usize| vec!["k"; parts].join(".");
        assert_eq!(passes_at(&format!("{} = 1\n", key(65))), None);
        assert_eq!(passes_at(&format!("{} = 1\n", key(66))), Some("1"));
        assert_eq!(passes_at(&format!("{} = []\n", key(64))), None);
        assert_eq!(passes_at(&format!("{} = []\n", key(65))), Some("["));
    }

    /// The headers `[[ts.a]]`, `[[ts.a.a]]` and so on up to `parts` parts after `ts`, each spelled quoted when `alternate` is set and its index is even.
    fn headers(parts: usize, alternate: bool) -> String {
        (1..=parts)
            .map(|count| {
                let quoted = alternate && count % 2 == 0;
                let names = std::iter::once("ts").chain(std::iter::repeat_n("a", count));
                let names: Vec<String> = names
                    .map(|name| {
                        if quoted {
                            format!("\"{name}\"")
                        } else {
                            name.to_owned()
                        }
                    })
                    .collect();
                format!("[[{}]]\n", names.join("."))
            })
            .collect()
    }

    #[test]
    fn an_array_of_tables_counts_twice_wherever_a_header_goes_through_it() {
        let flagged =
            (1..=40).find(|&parts| first_past_the_limit(&headers(parts, false)).is_some());
        // `ts` is a table and each `a` an array of tables: a header of k parts costs 2k - 1, which passes 64 at 33 parts, 32 of them after `ts`.
        assert_eq!(flagged, Some(32));
    }

    #[test]
    fn parts_are_compared_as_decoded() {
        let flagged = (1..=40).find(|&parts| first_past_the_limit(&headers(parts, true)).is_some());
        assert_eq!(flagged, Some(32));
    }

    #[test]
    fn a_syntax_error_is_not_a_nesting_error() {
        assert_eq!(passes_at("a = [1, \n[b\n"), None);
        assert_eq!(passes_at("= = ]]\n"), None);
    }

    #[test]
    fn a_hundred_thousand_arrays_are_skipped_not_followed() {
        let text = format!("a = {}", "[".repeat(100_000));
        let range = first_past_the_limit(&text).unwrap();
        assert_eq!(range.start, "a = ".len() + 64);
    }
}
