//! The `table` functions whose native loop is as long as the config says.
//!
//! No limit on instructions or on time interrupts a native call, and these functions loop once per element of a range or of a table that the config sizes at will: `table.move({}, 1, 2^40, 2)` walks two to the fortieth empty slots without running a Lua instruction, and so does `table.insert(t, 1, x)` on a table of that length, whether a `__len` says so or a handful of far-apart keys do. Each function here looks at its arguments before it runs and refuses a call whose loop would be longer than [`MAX_NODES`], the number of values the gsttt module takes from a config anyway.
//!
//! The length that `insert`, `remove`, `sort` and `concat` read is the one the check reads, because a table with a `__len` is refused: the function would ask the metamethod again after the check, and an answer that changes between the two questions would pass the check and still start the loop. Without a `__len` the length is the border of the table, which nothing can change between the check and the call.
//!
//! The other functions of `table` and `string` end by themselves: `unpack`, `string.byte`, `string.char` and `utf8.codepoint` stop at the limit of the Lua stack before they loop, and the rest copy or build at most as much as the memory limit allows.

use mlua::{Function, Lua, Table, Value, Variadic};

use super::{integer, located, raise};
use crate::value::MAX_NODES;

/// Why a call is refused: the argument it is about, and what is wrong with it.
struct Refusal {
    argument: usize,
    reason: String,
}

impl Refusal {
    fn new(argument: usize, reason: impl Into<String>) -> Self {
        Self {
            argument,
            reason: reason.into(),
        }
    }
}

/// What a function checks in its arguments before it runs. `None` lets the call through, including a call that is wrong in some other way: the original says what.
type Check = fn(&Lua, &[Value]) -> Option<Refusal>;

/// Puts a check in front of each function of `table` that loops as long as its arguments say.
pub(super) fn bound(lua: &Lua) -> mlua::Result<()> {
    let table: Table = lua.globals().get("table")?;
    let checks: [(&'static str, Check); 5] = [
        ("concat", concat),
        ("insert", insert),
        ("move", move_range),
        ("remove", remove),
        ("sort", sort),
    ];
    for (name, check) in checks {
        let real: Function = table.get(name)?;
        table.raw_set(name, checked(lua, name, real, check)?)?;
    }
    Ok(())
}

fn checked(lua: &Lua, name: &'static str, real: Function, check: Check) -> mlua::Result<Function> {
    lua.create_function(move |lua, arguments: Variadic<Value>| {
        if let Some(Refusal { argument, reason }) = check(lua, &arguments) {
            return Err(raise(
                lua,
                format!("bad argument #{argument} to '{name}' ({reason})"),
            ));
        }
        real.call::<Variadic<Value>>(arguments)
            .map_err(|error| located(lua, name, error))
    })
}

/// `table.move(from_table, first, last, target [, to_table])` copies `last - first + 1` elements.
fn move_range(lua: &Lua, arguments: &[Value]) -> Option<Refusal> {
    let [_, first, last, ..] = arguments else {
        return None;
    };
    range_refusal(3, integer(lua, first)?, integer(lua, last)?)
}

/// `table.concat(list [, separator [, first [, last]]])` joins `first` to `last`. `first` is 1 and `last` is the length of the list when they are not given, so either end can make the range long.
fn concat(lua: &Lua, arguments: &[Value]) -> Option<Refusal> {
    let [Value::Table(list), rest @ ..] = arguments else {
        return None;
    };
    let first = match rest.get(1) {
        None | Some(Value::Nil) => 1,
        Some(first) => integer(lua, first)?,
    };
    match rest.get(2) {
        None | Some(Value::Nil) => length_refusal(list)
            .or_else(|| range_refusal(3, first, i64::try_from(list.raw_len()).ok()?)),
        Some(last) => range_refusal(4, first, integer(lua, last)?),
    }
}

/// `table.insert(list, position, value)` shifts the elements from `position` up. With two arguments it only appends, which is one step.
fn insert(_: &Lua, arguments: &[Value]) -> Option<Refusal> {
    let [Value::Table(list), _, _] = arguments else {
        return None;
    };
    length_refusal(list)
}

/// `table.remove(list, position)` shifts the elements above `position` down. Without a position it takes the last element, which is one step.
fn remove(_: &Lua, arguments: &[Value]) -> Option<Refusal> {
    let [Value::Table(list), position, ..] = arguments else {
        return None;
    };
    if position.is_nil() {
        return None;
    }
    length_refusal(list)
}

/// `table.sort(list [, order])` visits the elements of the list.
fn sort(_: &Lua, arguments: &[Value]) -> Option<Refusal> {
    let [Value::Table(list), ..] = arguments else {
        return None;
    };
    length_refusal(list)
}

fn range_refusal(argument: usize, first: i64, last: i64) -> Option<Refusal> {
    too_long(first, last).then(|| {
        Refusal::new(
            argument,
            format!("the range holds more than {MAX_NODES} elements"),
        )
    })
}

fn length_refusal(list: &Table) -> Option<Refusal> {
    if has_length_metamethod(list) {
        return Some(Refusal::new(
            1,
            "a table with a `__len` metamethod is not accepted",
        ));
    }
    (list.raw_len() > MAX_NODES)
        .then(|| Refusal::new(1, format!("the table is longer than {MAX_NODES} elements")))
}

/// Whether the list has a `__len` of its own. Raw, so that no metamethod runs while the check looks.
fn has_length_metamethod(list: &Table) -> bool {
    list.metatable()
        .is_some_and(|meta| !matches!(meta.raw_get::<Value>("__len"), Ok(Value::Nil)))
}

/// Whether the range `first..=last` holds more than [`MAX_NODES`] elements. A range that ends before it begins holds none, and one so long that its length does not fit in an integer holds too many.
fn too_long(first: i64, last: i64) -> bool {
    last >= first
        && last
            .checked_sub(first)
            .and_then(|steps| usize::try_from(steps).ok())
            .is_none_or(|steps| steps >= MAX_NODES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_too_long_from_one_element_past_the_budget() {
        let budget = i64::try_from(MAX_NODES).unwrap();
        assert!(!too_long(1, budget));
        assert!(too_long(1, budget + 1));
        assert!(!too_long(0, budget - 1));
        assert!(!too_long(-budget / 2, budget / 2 - 1));
        assert!(too_long(-budget / 2, budget / 2));
    }

    #[test]
    fn a_range_that_ends_before_it_begins_holds_nothing() {
        assert!(!too_long(5, 4));
        assert!(!too_long(i64::MAX, i64::MIN));
    }

    #[test]
    fn a_range_whose_length_overflows_is_too_long() {
        assert!(too_long(i64::MIN, i64::MAX));
        assert!(too_long(-1, i64::MAX));
        assert!(too_long(i64::MIN, 0));
    }
}
