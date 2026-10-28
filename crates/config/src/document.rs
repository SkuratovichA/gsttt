//! A TOML file read as data: the steps every file of the config goes through before a typed value is read from it, and the error they share.
//!
//! The steps run in this order, and each refuses what the next one cannot take: the nesting limit, counted on the events of the parser ([`crate::depth`]); the syntax, with a span for every key and value; and a walk that refuses the values that have no JSON form. Typed reading of the parts comes after, in the callers.

use std::fmt::Write as _;
use std::ops::Range;

use gsttt_core::text::{LineIndex, with_path};
use serde::Deserialize;
use toml::Spanned;
use toml::de::{DeTable, DeValue, Deserializer};

use crate::depth::{MAX_DEPTH, first_past_the_limit};
use crate::text::{cut, log_line};

/// The longest error gsttt shows for a file of the config, in bytes: the file, the position and the message together.
const MESSAGE_LIMIT: usize = 1024;

/// Why a file does not read, and where in it, as a range of bytes. It becomes `<file>:<line>:<column>: <message>` where it leaves the crate, because only there is the text of the file at hand to count lines in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Located {
    pub(crate) message: String,
    pub(crate) span: Option<Range<usize>>,
}

impl Located {
    pub(crate) fn new(message: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
        }
    }

    /// `<file>:<line>:<column>: <message>`, with the line and the column counted from 1 and the column in characters, cut to 1 KiB. `text` is what `file` holds. An error without a place is `<file>: <message>`.
    pub(crate) fn render(&self, file: &str, text: &str) -> String {
        // The message holds text of the file, a key or a value, and the name of the file is the user's text as well: either may hold anything, so both are made fit for a line first.
        let message = log_line(&self.message);
        let file = log_line(file);
        let line = match &self.span {
            Some(span) => {
                let offset = u32::try_from(span.start).unwrap_or(u32::MAX);
                let index = LineIndex::new(text);
                format!(
                    "{file}:{}:{}: {message}",
                    index.line(offset),
                    index.column(text, offset)
                )
            }
            None => format!("{file}: {message}"),
        };
        cut(line, MESSAGE_LIMIT)
    }
}

impl From<toml::de::Error> for Located {
    fn from(error: toml::de::Error) -> Self {
        Self {
            message: error.message().to_owned(),
            span: error.span(),
        }
    }
}

/// A value that does not read into the type asked for, with the path to it in the message: `` `external[0].cmd`: invalid type: string "x", expected a sequence ``.
impl From<serde_path_to_error::Error<toml::de::Error>> for Located {
    fn from(error: serde_path_to_error::Error<toml::de::Error>) -> Self {
        let path = error.path().clone();
        let located = Self::from(error.into_inner());
        Self {
            message: with_path(&path, &located.message),
            span: located.span,
        }
    }
}

/// Parses `text` into a table that keeps the span of every key and value, after the nesting is within [`MAX_DEPTH`] and with no value that has no JSON form in it.
pub(crate) fn parse(text: &str) -> Result<Spanned<DeTable<'_>>, Located> {
    if let Some(span) = first_past_the_limit(text) {
        return Err(Located::new(
            format!("tables and arrays nest deeper than the limit of {MAX_DEPTH}"),
            span,
        ));
    }
    let table = DeTable::parse(text)?;
    let mut steps = Vec::new();
    for (key, value) in table.get_ref() {
        steps.push(Step::Key(key.get_ref()));
        refuse_without_json_form(&mut steps, value)?;
        steps.pop();
    }
    Ok(table)
}

/// `table`, as parsed by [`parse`], read into `T`. An error names the path to the value that did not fit, and where it is in the file.
pub(crate) fn read_typed<'de, T: Deserialize<'de>>(
    table: Spanned<DeTable<'de>>,
) -> Result<T, Located> {
    serde_path_to_error::deserialize(Deserializer::from(table)).map_err(Located::from)
}

/// Where a value sits, one step down from its container.
enum Step<'k> {
    Key(&'k str),
    Index(usize),
}

/// A date or a time, and a float that is not finite, have no JSON form: through serde a date would reach an options table as the private map `toml` makes of it, and a NaN as `null`. Nothing in the config or in a pack takes one, so they are refused wherever they are, with the key that holds them.
fn refuse_without_json_form<'k>(
    steps: &mut Vec<Step<'k>>,
    value: &'k Spanned<DeValue<'_>>,
) -> Result<(), Located> {
    let what = match value.get_ref() {
        DeValue::Datetime(_) => "a date or a time",
        DeValue::Float(float) if !float.as_str().parse::<f64>().is_ok_and(f64::is_finite) => {
            "a float that is not finite"
        }
        DeValue::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                steps.push(Step::Index(index));
                refuse_without_json_form(steps, item)?;
                steps.pop();
            }
            return Ok(());
        }
        DeValue::Table(table) => {
            for (key, child) in table {
                steps.push(Step::Key(key.get_ref()));
                refuse_without_json_form(steps, child)?;
                steps.pop();
            }
            return Ok(());
        }
        DeValue::String(_) | DeValue::Integer(_) | DeValue::Float(_) | DeValue::Boolean(_) => {
            return Ok(());
        }
    };
    Err(Located::new(
        format!("`{}` is {what}, which has no JSON form", dotted(steps)),
        value.span(),
    ))
}

/// The keys down to a value as the config writes them: `ts.paths[0]`.
fn dotted(steps: &[Step<'_>]) -> String {
    let mut path = String::new();
    for step in steps {
        match step {
            Step::Key(key) if path.is_empty() => path.push_str(key),
            Step::Key(key) => {
                path.push('.');
                path.push_str(key);
            }
            Step::Index(index) => {
                // Writing to a string cannot fail.
                let _ = write!(path, "[{index}]");
            }
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(text: &str) -> String {
        parse(text).unwrap_err().render("a.toml", text)
    }

    #[test]
    fn an_error_is_the_file_the_line_and_the_column() {
        assert_eq!(
            failure("a = 1\nb = \"ä\" 2\n"),
            "a.toml:2:9: unexpected key or value, expected newline, `#`"
        );
    }

    #[test]
    fn a_column_counts_characters() {
        // The `?` is the 7th character of the line and the 8th byte.
        let message = failure("a = 1\n\"ä\" = ?\n");
        assert!(message.starts_with("a.toml:2:7: "), "{message}");
    }

    #[test]
    fn a_value_without_a_json_form_is_named_by_its_key_and_line() {
        let dates = failure("[ts]\nsince = 1979-05-27\n");
        assert!(dates.starts_with("a.toml:2:9: "), "{dates}");
        assert!(
            dates.contains("`ts.since`") && dates.contains("no JSON form"),
            "{dates}"
        );
        for float in ["nan", "-nan", "inf", "-inf", "1e999"] {
            let message = failure(&format!("[ts]\nlimit = {float}\n"));
            assert!(message.contains("`ts.limit`"), "{float}: {message}");
        }
        let nested = failure("[ts]\nlist = [[1], { a = [2, 12:30:00] }]\n");
        assert!(nested.contains("`ts.list[1].a[1]`"), "{nested}");
    }

    #[test]
    fn finite_numbers_are_read() {
        assert!(parse("a = 1.5\nb = 1e3\nc = -0.0\nd = 0x10\ne = 1_000\n").is_ok());
    }

    #[test]
    fn nesting_past_the_limit_is_refused_with_the_limit_and_the_place() {
        let text = format!("a = {}", "[".repeat(100_000));
        let message = failure(&text);
        assert!(message.starts_with("a.toml:1:69: "), "{message}");
        assert!(message.contains("limit of 64"), "{message}");
    }

    #[test]
    fn a_long_error_is_cut_to_a_kilobyte() {
        let text = format!("\"{}\" = ", "k".repeat(100_000));
        let long = Located::new("k".repeat(100_000), 0..1).render("a.toml", &text);
        assert_eq!(long.len(), 1024);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn an_error_without_a_place_names_only_the_file() {
        let error = Located {
            message: "oops".to_owned(),
            span: None,
        };
        assert_eq!(error.render("a.toml", ""), "a.toml: oops");
    }
}
