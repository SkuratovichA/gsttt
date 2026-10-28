//! Lua values to JSON: how the tables a config hands to the gsttt module become the options, packs and externals of the setup.
//!
//! A table whose keys are exactly `1` to `n` is a list, an empty table is an empty list, and any other table is a map. A map's keys are sorted before they are emitted: Lua walks a table in an order that depends on a hash seed that differs between states, and the setup must not.

use mlua::{Table, Value};
use serde_json::{Map, Number, Value as Json};

/// Tables nested deeper than this are refused, which also stops a table that contains itself.
const MAX_DEPTH: usize = 64;

/// How many values one conversion may build. A table that is used twice is converted twice, so a few lines of Lua make a table that stands for billions of JSON values, and neither the memory limit nor the instruction limit of the sandbox sees what Rust builds from it.
const MAX_NODES: usize = 1_000_000;

/// Why a Lua value cannot be part of the setup. Every variant says where in the value the problem is.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ValueError {
    /// A function, a thread, userdata, or a key that is not a string or a number.
    #[error("{path}: {what} cannot be part of the setup")]
    Unsupported { path: String, what: String },
    #[error("{path}: {number} is not a finite number")]
    NotFinite { path: String, number: f64 },
    #[error("{path}: the string is not valid UTF-8")]
    NotUtf8 { path: String },
    #[error("{path}: tables nest deeper than {limit} levels, or contain themselves")]
    TooDeep { path: String, limit: usize },
    #[error(
        "{path}: the value holds more than {limit} values, a table that appears twice counting twice"
    )]
    TooLarge { path: String, limit: usize },
    #[error("{path}: two keys both become `{key}` in JSON")]
    DuplicateKey { path: String, key: String },
    #[error("{path}: {source}")]
    Lua { path: String, source: mlua::Error },
}

/// Reads `value` as JSON. `label` starts the path that errors report, such as `g.ts` in `g.ts.paths[2]`.
pub(crate) fn to_json(value: &Value, label: &str) -> Result<Json, ValueError> {
    let mut budget = MAX_NODES;
    convert(value, label, 0, &mut budget)
}

fn too_large(path: &str) -> ValueError {
    ValueError::TooLarge {
        path: path.to_owned(),
        limit: MAX_NODES,
    }
}

/// Converts `value` and everything below it. `budget` is how many more values may be built: each call takes one, and the call that finds it spent is the one that names its path.
fn convert(
    value: &Value,
    path: &str,
    depth: usize,
    budget: &mut usize,
) -> Result<Json, ValueError> {
    *budget = budget.checked_sub(1).ok_or_else(|| too_large(path))?;
    match value {
        Value::Nil => Ok(Json::Null),
        Value::Boolean(flag) => Ok(Json::Bool(*flag)),
        Value::Integer(number) => Ok(Json::from(*number)),
        Value::Number(number) => {
            finite(*number)
                .map(Json::Number)
                .ok_or_else(|| ValueError::NotFinite {
                    path: path.to_owned(),
                    number: *number,
                })
        }
        Value::String(text) => text
            .to_str()
            .map(|text| Json::String(String::from(&*text)))
            .map_err(|_| ValueError::NotUtf8 {
                path: path.to_owned(),
            }),
        Value::Table(table) => convert_table(table, path, depth, budget),
        other => Err(ValueError::Unsupported {
            path: path.to_owned(),
            what: format!("a {}", other.type_name()),
        }),
    }
}

/// A float without a fraction becomes an integer, so `1e3` and `1000` are the same JSON number and the same fingerprint: Rust prints such a float without a fraction or an exponent, and JSON reads that text back as an integer.
fn finite(number: f64) -> Option<Number> {
    if !number.is_finite() {
        return None;
    }
    number.to_string().parse().ok()
}

fn convert_table(
    table: &Table,
    path: &str,
    depth: usize,
    budget: &mut usize,
) -> Result<Json, ValueError> {
    if depth >= MAX_DEPTH {
        return Err(ValueError::TooDeep {
            path: path.to_owned(),
            limit: MAX_DEPTH,
        });
    }
    // The entries are collected before they are converted, so the budget is checked as they come: a table of millions of entries must not be copied whole just to be refused.
    let mut entries = Vec::new();
    for entry in table.pairs::<Value, Value>() {
        if entries.len() >= *budget {
            return Err(too_large(path));
        }
        entries.push(entry.map_err(|source| ValueError::Lua {
            path: path.to_owned(),
            source,
        })?);
    }
    let count = entries.len();
    // Keys are distinct, so `count` integer keys inside `1..=count` are exactly `1..=count`. An empty table passes too.
    let is_list = entries.iter().all(|(key, _)| {
        matches!(key, Value::Integer(index)
            if usize::try_from(*index).is_ok_and(|index| (1..=count).contains(&index)))
    });
    if is_list {
        list(entries, path, depth, budget)
    } else {
        map(entries, path, depth, budget)
    }
}

fn list(
    mut entries: Vec<(Value, Value)>,
    path: &str,
    depth: usize,
    budget: &mut usize,
) -> Result<Json, ValueError> {
    entries.sort_by_key(|(key, _)| key.as_integer());
    entries
        .iter()
        .enumerate()
        .map(|(position, (_, value))| {
            convert(
                value,
                &format!("{path}[{}]", position + 1),
                depth + 1,
                budget,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Json::Array)
}

fn map(
    entries: Vec<(Value, Value)>,
    path: &str,
    depth: usize,
    budget: &mut usize,
) -> Result<Json, ValueError> {
    let mut named = Vec::with_capacity(entries.len());
    for (key, value) in entries {
        named.push((key_text(&key, path)?, value));
    }
    named.sort_by(|(left, _), (right, _)| left.cmp(right));
    let mut object = Map::new();
    for (key, value) in named {
        let converted = convert(&value, &format!("{path}.{key}"), depth + 1, budget)?;
        if object.insert(key.clone(), converted).is_some() {
            return Err(ValueError::DuplicateKey {
                path: path.to_owned(),
                key,
            });
        }
    }
    Ok(Json::Object(object))
}

fn key_text(key: &Value, path: &str) -> Result<String, ValueError> {
    match key {
        Value::String(text) => {
            text.to_str()
                .map(|text| String::from(&*text))
                .map_err(|_| ValueError::NotUtf8 {
                    path: path.to_owned(),
                })
        }
        Value::Integer(number) => Ok(number.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        other => Err(ValueError::Unsupported {
            path: path.to_owned(),
            what: format!("a {} key", other.type_name()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use mlua::{Lua, LuaSerdeExt};
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    fn read(source: &str) -> Result<Json, ValueError> {
        let lua = Lua::new();
        let value: Value = lua.load(source).eval().unwrap();
        to_json(&value, "t")
    }

    /// The message for a value that must be refused. One that is accepted is reported without printing it, as it may be enormous.
    fn refused(source: &str) -> String {
        match read(source) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("the value was accepted: {source}"),
        }
    }

    #[test]
    fn keys_one_to_n_make_a_list_and_an_empty_table_an_empty_list() {
        assert_eq!(read(r#"return { "a", "b" }"#).unwrap(), json!(["a", "b"]));
        assert_eq!(
            read(r#"return { [2] = "b", [1] = "a" }"#).unwrap(),
            json!(["a", "b"])
        );
        assert_eq!(read("return {}").unwrap(), json!([]));
        assert_eq!(read("return { {}, { {} } }").unwrap(), json!([[], [[]]]));
    }

    #[test]
    fn any_other_table_is_a_map_with_sorted_keys() {
        assert_eq!(
            read("return { b = 1, a = 2, c = { 3 } }").unwrap(),
            json!({ "a": 2, "b": 1, "c": [3] })
        );
        // A gap, a start other than 1 and a mix of both kinds of key are maps with the keys written out.
        assert_eq!(
            read(r#"return { [1] = "a", [3] = "c" }"#).unwrap(),
            json!({ "1": "a", "3": "c" })
        );
        assert_eq!(
            read(r#"return { [0] = "z", "a" }"#).unwrap(),
            json!({ "0": "z", "1": "a" })
        );
        assert_eq!(
            read(r#"return { "a", b = 1 }"#).unwrap(),
            json!({ "1": "a", "b": 1 })
        );
    }

    #[test]
    fn a_float_without_a_fraction_is_an_integer() {
        assert_eq!(
            read("return { 1e3, 2.5, -0.25, 3.0 }").unwrap(),
            json!([1000, 2.5, -0.25, 3])
        );
        assert_eq!(
            read("return 2^53").unwrap(),
            json!(9_007_199_254_740_992_i64)
        );
    }

    #[test]
    fn what_cannot_be_json_is_refused_and_the_error_says_where() {
        let function = read(r#"return { conditions = { "import", print } }"#).unwrap_err();
        assert_eq!(
            function.to_string(),
            "t.conditions[2]: a function cannot be part of the setup"
        );
        let nan = read("return { size = 0/0 }").unwrap_err();
        assert_eq!(nan.to_string(), "t.size: NaN is not a finite number");
        let infinity = read("return { size = 1/0 }").unwrap_err();
        assert!(infinity.to_string().contains("not a finite number"));
        let bytes = read(r#"return { name = "\xff" }"#).unwrap_err();
        assert_eq!(bytes.to_string(), "t.name: the string is not valid UTF-8");
        let key = read("return { [true] = 1 }").unwrap_err();
        assert_eq!(
            key.to_string(),
            "t: a boolean key cannot be part of the setup"
        );
    }

    #[test]
    fn a_table_that_contains_itself_stops_at_the_depth_limit() {
        let error = read("local t = {} t.again = t return t").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("tables nest deeper than 64 levels"),
            "{error}"
        );
    }

    #[test]
    fn a_table_that_is_used_twice_per_level_is_stopped_by_the_budget() {
        // 21 levels are 2^21 leaves: far over the budget, and few enough that the unbudgeted code finishes.
        let message = refused(r#"local t = { "x" } for _ = 1, 21 do t = { t, t } end return t"#);
        assert!(
            message.starts_with("t[1]") && message.contains("more than 1000000 values"),
            "{message}"
        );
        // A table of that many entries is refused before it is copied, too.
        let flat = refused("local t = {} for i = 1, 1100000 do t[i] = i end return t");
        assert!(flat.contains("more than 1000000 values"), "{flat}");
        // Under the budget, a table used twice is simply converted twice.
        let shared = read("local a = { 1, 2 } return { a, a }").unwrap();
        assert_eq!(shared, json!([[1, 2], [1, 2]]));
    }

    #[test]
    fn two_keys_that_print_the_same_are_refused() {
        let error = read(r#"return { [1] = "a", ["1"] = "b", [3] = "c" }"#).unwrap_err();
        assert_eq!(error.to_string(), "t: two keys both become `1` in JSON");
    }

    fn json_values() -> impl Strategy<Value = Json> {
        let leaf = prop_oneof![
            any::<bool>().prop_map(Json::Bool),
            any::<i32>().prop_map(Json::from),
            "[a-z@*/_ -]{0,8}".prop_map(Json::String),
        ];
        leaf.prop_recursive(4, 32, 4, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..4).prop_map(Json::Array),
                prop::collection::btree_map("[a-z@*/_]{1,6}", inner, 1..4)
                    .prop_map(|entries| Json::Object(entries.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #[test]
        fn json_survives_the_trip_through_a_lua_table(expected in json_values()) {
            let lua = Lua::new();
            let table = lua.to_value(&expected).unwrap();
            prop_assert_eq!(to_json(&table, "t").unwrap(), expected);
        }
    }
}
