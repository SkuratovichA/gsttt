//! `setmetatable`, which gives a table a copy of the metatable it is passed.
//!
//! Lua reads `__gc` when `setmetatable` runs and `__mode` at every collection, and it runs finalizers and clears weak tables with the instruction hook off. A finalizer is a function that no limit stops, and weak tables whose entries hold on to each other take minutes to clear. `setmetatable` refuses a metatable with either field, but a metatable the config still holds could be given the field after the call.
//!
//! So the table gets a copy that nothing but the table points at. `getmetatable` answers with the table that was passed, or with its `__metatable` as Lua does, and `debug` is not in the state, so the config has no way to reach a table that Lua consults. The metatables the state installs itself are kept from the config with `__metatable`.
//!
//! What the config sees of that: a field written into a metatable after `setmetatable` is not in the metatable of the tables that got it, but a method added later to the table that `__index` names is found.

use mlua::{Function, Lua, Table, Value, Variadic};

use super::{located, raise, text_chunk};

const NAME: &str = "setmetatable";

/// The fields Lua reads outside the instruction limit, and why a metatable cannot have them.
const REFUSED: [(&str, &str); 2] = [
    ("__gc", "finalizers run outside the instruction limit"),
    (
        "__mode",
        "weak tables are cleared outside the instruction limit",
    ),
];

/// The copy is a loop in Lua so that the instruction limit counts it field by field: a native copy is one instruction however many fields the config gave the metatable, and the hook looks once in a thousand instructions. It walks with the raw `next`, which the config is not given.
const COPY: &str = r"
local next = ...
return function(meta)
  local copy = {}
  for key, value in next, meta do copy[key] = value end
  return copy
end
";

/// Replaces `setmetatable`. It runs before the globals are pruned, because the copy needs the `next` that is dropped with them.
pub(super) fn guard(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let original: Function = globals.get(NAME)?;
    let next: Function = globals.get("next")?;
    let copy_fields: Function = text_chunk(lua, "guards", COPY).call(next)?;
    // The key under which a copy says whether the config protected it. A table that nothing else holds, so that the config cannot write or read it.
    let mark = lua.create_table()?;
    let setmetatable = lua.create_function(move |lua, arguments: Variadic<Value>| {
        let (target, meta) = match arguments.as_slice() {
            [Value::Table(target), Value::Nil, ..] => (target, None),
            [Value::Table(target), Value::Table(meta), ..] => (target, Some(meta)),
            // The original refuses everything else before it changes anything, and words the refusal.
            _ => {
                return original
                    .call::<Table>(arguments.clone())
                    .map_err(|error| located(lua, NAME, error));
            }
        };
        if is_protected(&mark, target) {
            return Err(raise(lua, "cannot change a protected metatable"));
        }
        let Some(meta) = meta else {
            target.set_metatable(None)?;
            return Ok(target.clone());
        };
        for (field, reason) in REFUSED {
            if !meta.raw_get::<Value>(field)?.is_nil() {
                return Err(raise(
                    lua,
                    format!("a metatable cannot have `{field}`: {reason}"),
                ));
            }
        }
        let copy: Table = copy_fields.call(meta)?;
        // `getmetatable` gives back the `__metatable` of the config, or the table it passed, and a table with a `__metatable` of the config's own is protected.
        let given: Value = meta.raw_get("__metatable")?;
        let protected = !given.is_nil();
        let shown = if protected {
            given
        } else {
            Value::Table(meta.clone())
        };
        copy.raw_set("__metatable", shown)?;
        copy.raw_set(&mark, protected)?;
        target.set_metatable(Some(copy))?;
        Ok(target.clone())
    })?;
    globals.raw_set(NAME, setmetatable)
}

/// Whether `target` has a metatable that `setmetatable` leaves alone. Lua protects a metatable that has a `__metatable`, and every copy has one, so the copy says in `mark` whether the config wrote it. A metatable without the mark was installed by the state itself, and the config does not replace that.
fn is_protected(mark: &Table, target: &Table) -> bool {
    target.metatable().is_some_and(|installed| {
        !matches!(installed.raw_get::<Value>(mark), Ok(Value::Boolean(false)))
    })
}
