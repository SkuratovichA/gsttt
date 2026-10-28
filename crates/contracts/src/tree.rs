//! The YAML reader of the tree that OpenAPI documents and buf configs are read into: it builds the `gsttt_core::tree` that a JSON document is read into, so a document is read in one way whichever syntax it is written in.

use std::collections::HashSet;

use gsttt_core::tree::{Entry, MAX_DEPTH, SyntaxError, Value};
use saphyr::{LoadableYamlNode, MarkedYaml};

/// The YAML key whose value, a mapping or a sequence of mappings, is merged into the mapping that holds the key.
const MERGE_KEY: &str = "<<";

/// Reads the first document of a YAML text.
pub(crate) fn parse_yaml(text: &str) -> Result<Value, SyntaxError> {
    let documents = MarkedYaml::load_from_str(text).map_err(|error| SyntaxError {
        line: u32::try_from(error.marker().line())
            .unwrap_or(u32::MAX)
            .max(1),
        message: error.info().to_owned(),
    })?;
    Ok(documents
        .first()
        .map_or(Value::Other, |document| from_yaml(document, 0)))
}

fn from_yaml(node: &MarkedYaml<'_>, depth: usize) -> Value {
    if depth > MAX_DEPTH {
        return Value::Other;
    }
    if let Some(mapping) = node.data.as_mapping() {
        // A key that is not a string, a status code written `200:`, is dropped: no reader asks for one.
        let entries = mapping
            .iter()
            .filter_map(|(key, value)| {
                Some(Entry {
                    key: key.data.as_str()?.to_owned(),
                    line: u32::try_from(key.span.start.line())
                        .unwrap_or(u32::MAX)
                        .max(1),
                    value: from_yaml(value, depth + 1),
                })
            })
            .collect();
        return Value::Map(merge_keys(entries));
    }
    if let Some(sequence) = node.data.as_sequence() {
        return Value::Seq(
            sequence
                .iter()
                .map(|item| from_yaml(item, depth + 1))
                .collect(),
        );
    }
    if node.data.is_null() {
        return Value::Null;
    }
    node.data
        .as_str()
        .map_or(Value::Other, |text| Value::Str(text.to_owned()))
}

/// Resolves the `<<` merge keys of a mapping, which the YAML loader leaves as entries named `<<` that hold the mapping
/// they name. The entries of a merged mapping follow those the mapping sets itself and never replace one, and of
/// several merged mappings the first wins. A `<<` that names no mapping stays an entry.
fn merge_keys(entries: Vec<Entry>) -> Vec<Entry> {
    if !entries.iter().any(|entry| entry.key == MERGE_KEY) {
        return entries;
    }
    let mut own = Vec::with_capacity(entries.len());
    let mut sources: Vec<Vec<Entry>> = Vec::new();
    for entry in entries {
        if entry.key != MERGE_KEY {
            own.push(entry);
            continue;
        }
        match entry.value {
            Value::Map(merged) => sources.push(merged),
            Value::Seq(items) if items.iter().any(|item| matches!(item, Value::Map(_))) => {
                sources.extend(items.into_iter().filter_map(|item| match item {
                    Value::Map(merged) => Some(merged),
                    _ => None,
                }));
            }
            value => own.push(Entry {
                key: entry.key,
                line: entry.line,
                value,
            }),
        }
    }
    let mut taken: HashSet<String> = own.iter().map(|entry| entry.key.clone()).collect();
    for entry in sources.into_iter().flatten() {
        if taken.insert(entry.key.clone()) {
            own.push(entry);
        }
    }
    own
}

#[cfg(test)]
mod tests {
    use gsttt_core::fact::Line;
    use proptest::prelude::*;

    use super::*;

    // The same walk as `lines_of` in the tests of `crates/core/src/tree.rs`: a test module of another crate cannot be imported, so each crate keeps its copy.
    fn lines_of(value: &Value, out: &mut Vec<Line>) {
        match value {
            Value::Map(entries) => {
                for entry in entries {
                    out.push(entry.line);
                    lines_of(&entry.value, out);
                }
            }
            Value::Seq(items) => items.iter().for_each(|item| lines_of(item, out)),
            Value::Str(_) | Value::Null | Value::Other => {}
        }
    }

    #[test]
    fn yaml_keys_remember_their_lines() {
        let tree = parse_yaml("paths:\n  /a:\n    get: {}\n").unwrap();
        let paths = tree.get("paths").unwrap();
        assert_eq!(paths.entries()[0].key, "/a");
        assert_eq!(paths.entries()[0].line, 2);
        assert_eq!(paths.get("/a").unwrap().entries()[0].line, 3);
    }

    fn keys(value: &Value) -> Vec<(&str, Line)> {
        value
            .entries()
            .iter()
            .map(|entry| (entry.key.as_str(), entry.line))
            .collect()
    }

    #[test]
    fn an_alias_reads_as_the_node_it_names() {
        let tree = parse_yaml("base: &base\n  get: {}\nuse: *base\n").unwrap();
        assert_eq!(keys(tree.get("use").unwrap()), vec![("get", 2)]);
    }

    #[test]
    fn a_merge_key_adds_the_entries_of_the_mapping_it_names() {
        let tree = parse_yaml("base: &base\n  get: {}\nuse:\n  <<: *base\n  post: {}\n").unwrap();
        assert_eq!(
            keys(tree.get("use").unwrap()),
            vec![("post", 5), ("get", 2)]
        );
    }

    #[test]
    fn an_entry_a_mapping_sets_itself_wins_over_a_merged_one() {
        let text = "base: &base\n  get: { a: 1 }\n  put: {}\nuse:\n  <<: *base\n  get: {}\n";
        let tree = parse_yaml(text).unwrap();
        let used = tree.get("use").unwrap();
        assert_eq!(keys(used), vec![("get", 6), ("put", 3)]);
        assert_eq!(used.get("get"), Some(&Value::Map(Vec::new())));
    }

    #[test]
    fn the_first_of_several_merged_mappings_wins() {
        let text = "a: &a\n  get: { a: 1 }\nb: &b\n  get: {}\n  put: {}\nuse:\n  <<: [*a, *b]\n";
        let tree = parse_yaml(text).unwrap();
        let used = tree.get("use").unwrap();
        assert_eq!(keys(used), vec![("get", 2), ("put", 5)]);
        assert_eq!(used.get("get").map(|get| get.entries().len()), Some(1));
    }

    #[test]
    fn a_merge_key_that_names_no_mapping_stays_an_entry() {
        let tree = parse_yaml("use:\n  <<: text\n  get: {}\n").unwrap();
        assert_eq!(keys(tree.get("use").unwrap()), vec![("<<", 2), ("get", 3)]);
    }

    #[test]
    fn a_null_reads_as_null_whichever_way_it_is_written() {
        let tree = parse_yaml("a: null\nb: ~\nc:\nd: text\ne: 1\n").unwrap();
        let values: Vec<&Value> = tree.entries().iter().map(|entry| &entry.value).collect();
        assert_eq!(
            values,
            [
                &Value::Null,
                &Value::Null,
                &Value::Null,
                &Value::Str("text".to_owned()),
                &Value::Other,
            ]
        );
    }

    proptest! {
        #[test]
        fn lines_stay_inside_the_document(text in "[a-z0-9:\\-\\[\\]{}\", \n#&*<]{0,120}") {
            let count = Line::try_from(text.lines().count())
                .unwrap_or(Line::MAX)
                .saturating_add(1);
            match parse_yaml(&text) {
                Ok(tree) => {
                    let mut lines = Vec::new();
                    lines_of(&tree, &mut lines);
                    prop_assert!(
                        lines.iter().all(|line| (1..=count).contains(line)),
                        "{lines:?} of {count}"
                    );
                }
                Err(error) => prop_assert!(error.line >= 1),
            }
        }

        #[test]
        fn arbitrary_text_never_panics(text in "\\PC*") {
            drop(parse_yaml(&text));
        }
    }
}
