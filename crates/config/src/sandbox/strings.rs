//! The `string` library without its pattern matcher, and a `tostring` that keeps addresses out of a config.
//!
//! A Lua pattern can backtrack for ever inside one native call, and no limit on instructions or on time interrupts a native call, so `match`, `gmatch` and `gsub` are gone and `find` looks only for the text it is given. A config has no use for patterns: it matches names with globs and prefixes. The search itself is linear in the length of the subject, which the one in Lua is not when the text it looks for almost occurs everywhere.
//!
//! `rep` is the other native whose loop the script sizes: repeating nothing a quintillion times is a loop over no bytes that takes as long as the count, so it returns the empty string at once.
//!
//! An address, which `tostring` gives for a table or a function and `string.format("%p")` gives for anything, differs from run to run, so a config that lets one into its output no longer evaluates to the same setup twice.

use mlua::{FromLua, Function, Lua, LuaString, Table, Value, Variadic};

use super::{integer, located, raise};

/// Takes the pattern functions out of `string`, puts in a `find` that never reads patterns and a `format` that prints no address, and replaces the global `tostring`. Returns the new `tostring`, for the code that shows values (`print`).
pub(super) fn restrict(lua: &Lua) -> mlua::Result<Function> {
    let globals = lua.globals();
    let string: Table = globals.get("string")?;
    for name in ["match", "gmatch", "gsub"] {
        string.raw_set(name, Value::Nil)?;
    }
    string.raw_set("find", find(lua)?)?;

    let real_tostring: Function = globals.get("tostring")?;
    let tostring =
        lua.create_function(move |lua, value: Value| show(lua, &real_tostring, value))?;
    globals.raw_set("tostring", &tostring)?;

    let real_format: Function = string.get("format")?;
    string.raw_set("format", format(lua, real_format, tostring.clone())?)?;

    let real_rep: Function = string.get("rep")?;
    string.raw_set("rep", rep(lua, real_rep)?)?;
    Ok(tostring)
}

/// `string.rep(unit, count [, separator])`. A unit and a separator without bytes make a result without bytes, however often they are repeated, and the original still walks all `count` repetitions one after the other. Every other call goes to the original: a result with bytes is bounded by the memory limit, because the original allocates all of it first.
fn rep(lua: &Lua, real: Function) -> mlua::Result<Function> {
    lua.create_function(move |lua, arguments: Variadic<Value>| {
        if repeats_nothing(lua, &arguments) {
            return Ok(Variadic::from(vec![Value::String(lua.create_string("")?)]));
        }
        real.call::<Variadic<Value>>(arguments)
            .map_err(|error| located(lua, "rep", error))
    })
}

/// Whether `arguments` repeat a unit without bytes, with a separator without bytes, a number of times the original accepts. Any other argument is left to the original, which says what is wrong with it.
fn repeats_nothing(lua: &Lua, arguments: &[Value]) -> bool {
    let [unit, count, rest @ ..] = arguments else {
        return false;
    };
    is_empty_string(unit)
        && integer(lua, count).is_some()
        && rest
            .first()
            .is_none_or(|separator| separator.is_nil() || is_empty_string(separator))
}

fn is_empty_string(value: &Value) -> bool {
    matches!(value, Value::String(text) if text.as_bytes().is_empty())
}

/// `string.find(subject, text [, init [, plain]])` for a `text` that is never a pattern, so the fourth argument means nothing. The results are those of the original with `plain` set: the first and the last position of the match, or `nil`.
fn find(lua: &Lua) -> mlua::Result<Function> {
    lua.create_function(|lua, (subject, text, init): (Value, Value, Value)| {
        let subject: LuaString = argument(lua, 1, "string", subject)?;
        let text: LuaString = argument(lua, 2, "string", text)?;
        let init = if init.is_nil() {
            1
        } else {
            argument(lua, 3, "integer", init)?
        };
        Ok(Variadic::from(
            match locate(&subject.as_bytes(), &text.as_bytes(), init) {
                Some((first, last)) => vec![Value::Integer(first), Value::Integer(last)],
                None => vec![Value::Nil],
            },
        ))
    })
}

/// The argument at `position` of `find` read as a `T`, or the error Lua gives for a wrong one, with the position of the call.
fn argument<T: FromLua>(
    lua: &Lua,
    position: usize,
    expected: &str,
    value: Value,
) -> mlua::Result<T> {
    let got = value.type_name();
    T::from_lua(value, lua).map_err(|_| {
        raise(
            lua,
            format!("bad argument #{position} to 'find' ({expected} expected, got {got})"),
        )
    })
}

/// Where `text` first occurs in `subject` at or after `init`, as the first and the last position, counted from 1. `init` counts from the end when negative and from the start when it is 0, as in `string.find`.
fn locate(subject: &[u8], text: &[u8], init: i64) -> Option<(i64, i64)> {
    let length = i64::try_from(subject.len()).ok()?;
    let start = match init {
        1.. => init,
        0 => 1,
        _ if init < -length => 1,
        _ => length + init + 1,
    } - 1;
    // A start beyond the end finds nothing, not even the empty text.
    let rest = subject.get(usize::try_from(start).ok()?..)?;
    let offset = i64::try_from(memchr::memmem::find(rest, text)?).ok()?;
    let first = start + offset + 1;
    Some((first, first + i64::try_from(text.len()).ok()? - 1))
}

/// `tostring`, except that a value whose usual text is an address is called by its type: `table`, `function`.
fn show(lua: &Lua, real: &Function, value: Value) -> mlua::Result<Value> {
    if shows_address(&value) {
        return Ok(Value::String(lua.create_string(value.type_name())?));
    }
    real.call(value)
}

/// Whether the usual text of `value` is an address: a table, a function, a thread or userdata without a `__tostring` to say otherwise.
fn shows_address(value: &Value) -> bool {
    match value {
        Value::Table(table) => !table.metatable().is_some_and(|metatable| {
            metatable
                .raw_get::<Value>("__tostring")
                .is_ok_and(|method| !method.is_nil())
        }),
        Value::Function(_) | Value::Thread(_) | Value::UserData(_) => true,
        _ => false,
    }
}

/// `string.format` with two differences: `%p` is refused, and a value `%s` would show as an address is shown as `tostring` shows it.
fn format(lua: &Lua, real: Function, tostring: Function) -> mlua::Result<Function> {
    lua.create_function(
        move |lua, (pattern, arguments): (Value, Variadic<Value>)| {
            if let Value::String(text) = &pattern
                && prints_a_pointer(&text.as_bytes())
            {
                return Err(raise(
                    lua,
                    "bad argument #1 to 'format' (`%p` prints an address, which differs from run to run)",
                ));
            }
            let mut call = Vec::with_capacity(arguments.len() + 1);
            call.push(pattern);
            for argument in arguments {
                call.push(if shows_address(&argument) {
                    tostring.call(argument)?
                } else {
                    argument
                });
            }
            real.call::<Value>(Variadic::from(call))
                .map_err(|error| located(lua, "format", error))
        },
    )
}

/// Whether the format string holds a `%p` conversion: a `%`, then flags, a width and a precision, then `p`. A `%%` is a percent sign and ends the scan of its own conversion.
fn prints_a_pointer(format: &[u8]) -> bool {
    let mut bytes = format.iter();
    while let Some(byte) = bytes.next() {
        if *byte == b'%'
            && bytes.find(|next| !matches!(**next, b'-' | b'+' | b' ' | b'#' | b'.' | b'0'..=b'9'))
                == Some(&b'p')
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn find_reads_positions_as_the_original_does() {
        // The cases the original documents: a start past the end, negative starts, the empty text.
        assert_eq!(locate(b"hello", b"ll", 1), Some((3, 4)));
        assert_eq!(locate(b"hello", b"l", 4), Some((4, 4)));
        assert_eq!(locate(b"hello", b"l", -2), Some((4, 4)));
        assert_eq!(locate(b"hello", b"h", -100), Some((1, 1)));
        assert_eq!(locate(b"hello", b"h", 0), Some((1, 1)));
        assert_eq!(locate(b"hello", b"", 6), Some((6, 5)));
        assert_eq!(locate(b"hello", b"", 7), None);
        assert_eq!(locate(b"hello", b"x", 1), None);
        assert_eq!(locate(b"", b"", 1), Some((1, 0)));
        assert_eq!(locate(b"hello", b"h", i64::MAX), None);
        assert_eq!(locate(b"hello", b"o", i64::MIN), Some((5, 5)));
    }

    proptest! {
        /// The original with `plain` set is the specification: the same results for every subject, text and start.
        #[test]
        fn find_agrees_with_the_original_plain_find(
            subject in "[ab.%]{0,12}",
            text in "[ab.%]{0,4}",
            init in -16i64..16,
        ) {
            let lua = Lua::new();
            let original: Function = lua
                .load("return function(s, p, init) return string.find(s, p, init, true) end")
                .eval()
                .unwrap();
            let expected: Variadic<Value> = original.call((subject.as_str(), text.as_str(), init)).unwrap();
            let expected: Vec<i64> = expected.iter().filter_map(Value::as_integer).collect();
            let actual: Vec<i64> = locate(subject.as_bytes(), text.as_bytes(), init)
                .map(|(first, last)| vec![first, last])
                .unwrap_or_default();
            prop_assert_eq!(actual, expected);
        }
    }

    #[test]
    fn a_format_string_with_a_pointer_is_recognised() {
        for format in ["%p", "x %p", "%5p", "%-5p", "%d %p", "%%%p", "%s%p"] {
            assert!(prints_a_pointer(format.as_bytes()), "{format}");
        }
        for format in ["", "p", "%%p", "%d%%p", "%s", "%5.2f", "100%", "%q p"] {
            assert!(!prints_a_pointer(format.as_bytes()), "{format}");
        }
    }
}
