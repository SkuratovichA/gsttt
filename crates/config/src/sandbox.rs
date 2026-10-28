//! The Lua state every config source runs in: `init.lua`, its local modules and the rule packs.
//!
//! The state has the pure libraries and the base functions that cannot reach outside it, a memory limit, an instruction limit and a `pairs` with a fixed order, so the same files always evaluate to the same setup.
//!
//! One gap is known and accepted: the instruction limit counts Lua instructions, so a pattern that backtracks for ever inside `string.find` is native code the limit cannot interrupt.

use std::cell::Cell;
use std::cmp::Ordering;
use std::fmt::Display;
use std::rc::Rc;

use mlua::chunk::{Chunk, ChunkMode};
use mlua::{Error, HookTriggers, Lua, LuaOptions, StdLib, Table, Value, Variadic, VmState};

/// Bytes of Lua memory a config may allocate.
pub(crate) const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Lua instructions a config may run.
pub(crate) const INSTRUCTION_LIMIT: u64 = 100_000_000;

/// Instructions between two looks at the budget. Small enough to stop a loop within microseconds, large enough to cost nothing.
const HOOK_INTERVAL: u32 = 1_000;

/// What the base library keeps. Everything else in the globals goes (`load`, `loadfile`, `dofile`, `collectgarbage` and whatever a newer Lua adds), because a list of what to remove misses the next one.
const KEPT_GLOBALS: &[&str] = &[
    "_G",
    "_VERSION",
    "assert",
    "error",
    "getmetatable",
    "ipairs",
    "math",
    "next",
    "pairs",
    "pcall",
    "print",
    "rawequal",
    "rawget",
    "rawlen",
    "rawset",
    "select",
    "setmetatable",
    "string",
    "table",
    "tonumber",
    "tostring",
    "type",
    "utf8",
    "warn",
    "xpcall",
];

/// Wraps the base functions through which a script could outlast its limits. It runs once, before the hook is set.
///
/// - `pcall` and `xpcall` re-raise the instruction limit when they catch it, so a loop around `pcall` still ends.
/// - `setmetatable` refuses `__gc`: Lua runs finalizers with hooks off, so a finalizer could loop for ever.
const GUARDS: &str = r#"
local check = ...
local rawget, real_pcall, real_xpcall, real_setmetatable, type, error = rawget, pcall, xpcall, setmetatable, type, error
function pcall(f, ...) return check(real_pcall(f, ...)) end
function xpcall(f, handler, ...) return check(real_xpcall(f, handler, ...)) end
function setmetatable(target, meta)
  if type(meta) == "table" and rawget(meta, "__gc") ~= nil then
    error("a metatable cannot have `__gc`: finalizers run outside the instruction limit", 2)
  end
  return real_setmetatable(target, meta)
end
"#;

/// The two limits of a state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) instructions: u64,
    pub(crate) memory: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            instructions: INSTRUCTION_LIMIT,
            memory: MEMORY_LIMIT,
        }
    }
}

/// How much of the instruction budget a run has used. Shared with the hook, which only sees `&Lua`.
#[derive(Debug, Default)]
struct Budget {
    executed: Cell<u64>,
    exhausted: Cell<bool>,
}

/// A Lua state with the limits above. One per evaluation: `init.lua`, the modules it requires and the packs it loads share it.
#[derive(Debug)]
pub(crate) struct Sandbox {
    lua: Lua,
    budget: Rc<Budget>,
    limits: Limits,
}

impl Sandbox {
    pub(crate) fn new() -> mlua::Result<Self> {
        Self::with_limits(Limits::default())
    }

    pub(crate) fn with_limits(limits: Limits) -> mlua::Result<Self> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )?;
        let budget = Rc::new(Budget::default());
        keep_safe_globals(&lua)?;
        route_output_to_log(&lua)?;
        order_pairs(&lua)?;
        guard_limits(&lua, &budget, limits)?;
        lua.set_memory_limit(limits.memory)?;
        let hook_budget = Rc::clone(&budget);
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
            move |_, _| {
                let executed = hook_budget
                    .executed
                    .get()
                    .saturating_add(u64::from(HOOK_INTERVAL));
                hook_budget.executed.set(executed);
                if executed > limits.instructions {
                    hook_budget.exhausted.set(true);
                    return Err(Error::runtime(instruction_message(limits.instructions)));
                }
                Ok(VmState::Continue)
            },
        )?;
        Ok(Self {
            lua,
            budget,
            limits,
        })
    }

    pub(crate) fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Starts a new run on the same state: the built-in packs share one state and each gets the whole budget.
    pub(crate) fn reset_budget(&self) {
        self.budget.executed.set(0);
        self.budget.exhausted.set(false);
    }

    pub(crate) fn load<'a>(&self, name: &str, source: &'a str) -> Chunk<'a> {
        text_chunk(&self.lua, name, source)
    }

    /// The message of a failed run: Lua's own (`name:3: message`), or the limit that stopped it.
    pub(crate) fn explain(&self, error: &Error) -> String {
        if self.budget.exhausted.get() {
            return instruction_message(self.limits.instructions);
        }
        describe(error, self.limits.memory)
    }
}

/// A text chunk named `name` in errors (`name:3: message`). Never a binary chunk: bytecode is not checked by Lua and can crash it.
pub(crate) fn text_chunk<'a>(lua: &Lua, name: &str, source: &'a str) -> Chunk<'a> {
    lua.load(source)
        .set_name(format!("@{name}"))
        .set_mode(ChunkMode::Text)
}

/// An error raised from a Rust callback, prefixed with the position of the Lua call that reached it the way Lua prefixes its own errors.
pub(crate) fn raise(lua: &Lua, message: impl Display) -> Error {
    let position = lua
        .inspect_stack(1, |frame| {
            let line = frame.current_line()?;
            let source = frame.source();
            Some(format!("{}:{line}: ", source.short_src.as_deref()?))
        })
        .flatten()
        .unwrap_or_default();
    Error::runtime(format!("{position}{message}"))
}

fn instruction_message(limit: u64) -> String {
    format!("stopped by the instruction limit of {limit} instructions")
}

fn memory_message(limit: usize) -> String {
    format!(
        "stopped by the memory limit of {} MiB",
        limit / (1024 * 1024)
    )
}

fn describe(error: &Error, memory: usize) -> String {
    match error {
        Error::CallbackError { cause, .. } => describe(cause, memory),
        // Lua's own allocation failures in the auxiliary library are plain string errors, not `MemoryError`.
        Error::MemoryError(_) => memory_message(memory),
        Error::RuntimeError(message) if message.contains("not enough memory") => {
            memory_message(memory)
        }
        Error::RuntimeError(message) | Error::SyntaxError { message, .. } => {
            without_traceback(message).to_owned()
        }
        other => other.to_string(),
    }
}

/// mlua appends a Lua traceback to the message of an error that began in Lua; the config's reader wants the message alone.
fn without_traceback(message: &str) -> &str {
    message
        .split_once("\nstack traceback:")
        .map_or(message, |(head, _)| head)
}

fn keep_safe_globals(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let keys = globals
        .pairs::<Value, Value>()
        .map(|pair| pair.map(|(key, _)| key))
        .collect::<mlua::Result<Vec<_>>>()?;
    for key in keys {
        let kept = matches!(
            &key,
            Value::String(name) if name.to_str().is_ok_and(|name| KEPT_GLOBALS.contains(&&*name))
        );
        if !kept {
            globals.raw_set(key, Value::Nil)?;
        }
    }
    let math: Table = globals.get("math")?;
    math.raw_set("random", Value::Nil)?;
    math.raw_set("randomseed", Value::Nil)?;
    // Bytecode is never loaded, and nothing in a config needs to produce it.
    let string: Table = globals.get("string")?;
    string.raw_set("dump", Value::Nil)
}

fn route_output_to_log(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let print = lua.create_function(|_, words: Variadic<Value>| {
        let line = tab_separated(&words)?;
        tracing::info!(target: "gsttt_config::lua", "{line}");
        Ok(())
    })?;
    let warn = lua.create_function(|_, words: Variadic<Value>| {
        let line = tab_separated(&words)?;
        tracing::warn!(target: "gsttt_config::lua", "{line}");
        Ok(())
    })?;
    globals.raw_set("print", print)?;
    globals.raw_set("warn", warn)
}

fn tab_separated(words: &[Value]) -> mlua::Result<String> {
    let words = words
        .iter()
        .map(Value::to_string)
        .collect::<mlua::Result<Vec<_>>>()?;
    Ok(words.join("\t"))
}

fn guard_limits(lua: &Lua, budget: &Rc<Budget>, limits: Limits) -> mlua::Result<()> {
    let budget = Rc::clone(budget);
    let check = lua.create_function(move |_, results: Variadic<Value>| {
        if budget.exhausted.get() {
            return Err(Error::runtime(instruction_message(limits.instructions)));
        }
        Ok(results)
    })?;
    lua.load(GUARDS)
        .set_name("=guards")
        .set_mode(ChunkMode::Text)
        .call::<()>(check)
}

/// Replaces `pairs` with one that visits the keys `1..n` in order and then the others sorted: numbers, then strings by bytes, then booleans. Lua's own order depends on string hashes, which change from run to run.
///
/// `next` is not replaced, so a script that calls it by hand still sees Lua's order; configs have no reason to.
fn order_pairs(lua: &Lua) -> mlua::Result<()> {
    let pairs = lua.create_function(|lua, subject: Value| {
        let Value::Table(table) = subject else {
            return Err(raise(
                lua,
                format!(
                    "bad argument #1 to 'pairs' (table expected, got {})",
                    subject.type_name()
                ),
            ));
        };
        let keys = ordered_keys(lua, &table)?;
        let mut position = 0;
        let step = lua.create_function_mut(move |_, _: Variadic<Value>| {
            // A key whose value was set to nil during the walk is skipped, as `next` would.
            while let Some(key) = keys.get(position) {
                position += 1;
                let value: Value = table.raw_get(key.clone())?;
                if !value.is_nil() {
                    return Ok(Variadic::from(vec![key.clone(), value]));
                }
            }
            Ok(Variadic::new())
        })?;
        Ok((step, Value::Nil, Value::Nil))
    })?;
    lua.globals().raw_set("pairs", pairs)
}

/// How the keys after the list part are ordered: by the derived order of the variants, then the value.
#[derive(Debug, PartialEq, PartialOrd)]
enum SortKey {
    Number(f64),
    Text(Vec<u8>),
    Flag(bool),
}

#[expect(
    clippy::cast_precision_loss,
    reason = "integer keys beyond 2^53 tie in the comparison, and a tie keeps Lua's own order, which is stable for integers"
)]
fn sort_key(key: &Value) -> Option<SortKey> {
    match key {
        Value::Integer(number) => Some(SortKey::Number(*number as f64)),
        Value::Number(number) => Some(SortKey::Number(*number)),
        Value::String(text) => Some(SortKey::Text(text.as_bytes().to_vec())),
        Value::Boolean(flag) => Some(SortKey::Flag(*flag)),
        _ => None,
    }
}

fn ordered_keys(lua: &Lua, table: &Table) -> mlua::Result<Vec<Value>> {
    let mut keys = Vec::new();
    let mut length: i64 = 0;
    loop {
        let next = length + 1;
        let value: Value = table.raw_get(next)?;
        if value.is_nil() {
            break;
        }
        keys.push(Value::Integer(next));
        length = next;
    }
    let mut rest = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair?;
        if matches!(&key, Value::Integer(index) if (1..=length).contains(index)) {
            continue;
        }
        let Some(order) = sort_key(&key) else {
            return Err(raise(
                lua,
                format!(
                    "pairs cannot order a key of type `{}`: use string, number or boolean keys",
                    key.type_name()
                ),
            ));
        };
        rest.push((order, key));
    }
    // A NaN key cannot exist, so the comparison is total; the fallback only satisfies the type.
    rest.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    keys.extend(rest.into_iter().map(|(_, key)| key));
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};

    use super::*;

    fn eval(sandbox: &Sandbox, source: &str) -> Result<String, String> {
        sandbox
            .load("t.lua", source)
            .eval::<Value>()
            .map_err(|error| sandbox.explain(&error))
            .and_then(|value| value.to_string().map_err(|error| error.to_string()))
    }

    fn small(instructions: u64, mebibytes: usize) -> Sandbox {
        Sandbox::with_limits(Limits {
            instructions,
            memory: mebibytes * 1024 * 1024,
        })
        .unwrap()
    }

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn limits_default_to_the_spec() {
        assert_eq!(
            Limits::default(),
            Limits {
                instructions: 100_000_000,
                memory: 64 * 1024 * 1024
            }
        );
    }

    #[test]
    fn only_the_pure_libraries_and_the_safe_base_functions_remain() {
        let sandbox = Sandbox::new().unwrap();
        let gone = [
            "io",
            "os",
            "package",
            "debug",
            "coroutine",
            "load",
            "loadfile",
            "dofile",
            "collectgarbage",
            "require",
            "math.random",
            "math.randomseed",
            "string.dump",
        ];
        for name in gone {
            assert_eq!(
                eval(&sandbox, &format!("return type({name})")).unwrap(),
                "nil",
                "{name}"
            );
        }
        for name in ["table", "string", "math", "utf8"] {
            assert_eq!(
                eval(&sandbox, &format!("return type({name})")).unwrap(),
                "table"
            );
        }
        for name in [
            "print",
            "pairs",
            "ipairs",
            "next",
            "pcall",
            "xpcall",
            "assert",
            "error",
            "select",
            "tostring",
            "tonumber",
            "type",
            "setmetatable",
            "getmetatable",
            "rawget",
            "rawset",
            "rawequal",
            "rawlen",
            "warn",
        ] {
            assert_eq!(
                eval(&sandbox, &format!("return type({name})")).unwrap(),
                "function",
                "{name}"
            );
        }
    }

    #[test]
    fn print_and_warn_write_to_the_log() {
        let capture = Capture::default();
        let writer = capture.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let sandbox = Sandbox::new().unwrap();
            eval(&sandbox, r#"print("hello", 1, true) warn("careful")"#).unwrap();
        });
        let log = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("hello\t1\ttrue"), "{log}");
        assert!(log.contains("careful"), "{log}");
    }

    #[test]
    fn an_error_starts_with_the_chunk_name_and_line() {
        let sandbox = Sandbox::new().unwrap();
        let message = eval(&sandbox, "local x = nil\nx()").unwrap_err();
        assert!(message.starts_with("t.lua:2: "), "{message}");
        assert!(!message.contains("stack traceback"), "{message}");
        let message = eval(&sandbox, "return +").unwrap_err();
        assert!(message.starts_with("t.lua:1: "), "{message}");
    }

    #[test]
    fn bytecode_is_not_loaded() {
        let sandbox = Sandbox::new().unwrap();
        assert!(eval(&sandbox, "\u{1b}Lua").is_err());
    }

    #[test]
    fn pairs_visits_the_list_in_order_and_then_the_other_keys_sorted() {
        let sandbox = Sandbox::new().unwrap();
        let walk = r#"
            local seen = {}
            for key in pairs(subject) do seen[#seen + 1] = tostring(key) end
            return table.concat(seen, ",")
        "#;
        let mixed = format!(
            r#"local subject = {{ 10, 20, [5] = "five", x = 1, b = 2, a = 3, [true] = 4, [2.5] = 5 }} {walk}"#
        );
        assert_eq!(eval(&sandbox, &mixed).unwrap(), "1,2,2.5,5,a,b,x,true");
        let letters = format!("local subject = {{ b = 1, a = 2, c = 3 }} {walk}");
        assert_eq!(eval(&sandbox, &letters).unwrap(), "a,b,c");
        let emptied = r#"
            local subject = { a = 1, b = 2, c = 3 }
            local seen = {}
            for key in pairs(subject) do subject.b = nil seen[#seen + 1] = key end
            return table.concat(seen, ",")
        "#;
        assert_eq!(eval(&sandbox, emptied).unwrap(), "a,c");
    }

    #[test]
    fn pairs_refuses_what_it_cannot_order_and_says_where() {
        let sandbox = Sandbox::new().unwrap();
        let message = eval(&sandbox, "for _ in pairs({ [{}] = 1 }) do end").unwrap_err();
        assert!(
            message.starts_with("t.lua:1: ") && message.contains("`table`"),
            "{message}"
        );
        let message = eval(&sandbox, "pairs(nil)").unwrap_err();
        assert!(
            message.starts_with("t.lua:1: ") && message.contains("pairs"),
            "{message}"
        );
    }

    #[test]
    fn the_order_of_pairs_is_the_same_in_every_state() {
        let source = r#"
            local subject = {}
            for _, key in ipairs({ "q", "w", "e", "r", "t", "y", "u", "i", "o", "p" }) do subject[key] = true end
            local seen = {}
            for key in pairs(subject) do seen[#seen + 1] = key end
            return table.concat(seen)
        "#;
        for _ in 0..20 {
            let sandbox = Sandbox::new().unwrap();
            assert_eq!(eval(&sandbox, source).unwrap(), "eiopqrtuwy");
        }
    }

    #[test]
    fn a_runaway_loop_stops_at_the_instruction_limit_even_inside_pcall() {
        let sandbox = small(300_000, 64);
        let loops = [
            "while true do end",
            "while true do pcall(function() while true do end end) end",
            "xpcall(function() while true do end end, print) while true do end",
            "local n = 0 repeat n = n + 1 until false",
        ];
        for source in loops {
            sandbox.reset_budget();
            let message = eval(&sandbox, source).unwrap_err();
            assert!(message.contains("instruction limit"), "{source}: {message}");
        }
        sandbox.reset_budget();
        assert_eq!(eval(&sandbox, "return 1 + 1").unwrap(), "2");
    }

    #[test]
    fn a_state_that_allocates_too_much_stops_at_the_memory_limit() {
        let sandbox = small(100_000_000, 8);
        for source in [
            r#"return #string.rep("x", 1e9)"#,
            "local t = {} for i = 1, 1e9 do t[i] = i end",
            r#"local s = "x" while true do s = s .. s end"#,
        ] {
            let message = eval(&sandbox, source).unwrap_err();
            assert!(
                message.contains("memory limit of 8 MiB"),
                "{source}: {message}"
            );
        }
        let real = Sandbox::new().unwrap();
        let message = eval(&real, r#"return #string.rep("x", 1e9)"#).unwrap_err();
        assert!(message.contains("memory limit of 64 MiB"), "{message}");
    }

    #[test]
    fn a_finalizer_cannot_escape_the_instruction_limit() {
        let sandbox = Sandbox::new().unwrap();
        let message = eval(
            &sandbox,
            "local subject = setmetatable({}, { __gc = function() end }) return subject",
        )
        .unwrap_err();
        assert!(
            message.starts_with("t.lua:1: ") && message.contains("__gc"),
            "{message}"
        );
        assert_eq!(
            eval(&sandbox, "return type(setmetatable({}, { __index = {} }))").unwrap(),
            "table"
        );
    }
}
