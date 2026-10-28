//! The Lua state every config source runs in: `init.lua`, its local modules and the rule packs.
//!
//! The state has the pure libraries and the base functions that cannot reach outside it, a memory limit, an instruction limit, a time limit and a `pairs` with a fixed order, so the same files always evaluate to the same setup.
//!
//! The instruction limit counts Lua instructions, and a native call is one instruction however long it runs. Two things make that safe. The time limit is checked with the instruction count, so a loop of native calls that each cost a little still stops. And the native calls that could run for ever have no way to: the pattern matcher of `string` is not in the state (see `strings.rs`), and the functions that loop as long as an argument says refuse a long loop before they start (`rep` in `strings.rs`, the `table` functions in `tables.rs`).
//!
//! The collector is the other work that no limit interrupts: Lua runs finalizers and clears weak tables with the hook off. A config cannot ask for either, because the metatable a table gets from `setmetatable` is a copy without `__gc` and `__mode` that the config cannot write into (see `metatables.rs`).

mod metatables;
mod output;
mod strings;
mod tables;

use std::cell::Cell;
use std::cmp::Ordering;
use std::fmt::Display;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::chunk::{Chunk, ChunkMode};
use mlua::{Error, HookTriggers, Lua, LuaOptions, StdLib, Table, Value, Variadic, VmState};

use crate::text::cut;

/// Bytes of Lua memory a config may allocate.
pub(crate) const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Lua instructions a config may run.
pub(crate) const INSTRUCTION_LIMIT: u64 = 100_000_000;

/// Time a config may run, from the start of its evaluation. A real config takes microseconds; this is what a loop of native calls (a `string.rep` of megabytes, a `pairs` over a huge table) gets before it is stopped, a loop the instruction limit lets run for hours.
pub(crate) const TIME_LIMIT: Duration = Duration::from_secs(2);

/// The longest error message a failed run hands to its caller, in bytes. The text of an error is the config's own and can be as long as it likes.
const MESSAGE_LIMIT: usize = 1024;

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

/// Wraps `pcall` and `xpcall`, through which a script could outlast its limits: they re-raise the instruction limit when they catch it, so a loop around `pcall` still ends. It runs once, before the hook is set.
const GUARDS: &str = r"
local check = ...
local real_pcall, real_xpcall = pcall, xpcall
function pcall(f, ...) return check(real_pcall(f, ...)) end
function xpcall(f, handler, ...) return check(real_xpcall(f, handler, ...)) end
";

/// The limits of a state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) instructions: u64,
    pub(crate) memory: usize,
    /// The time a run may take. A field, so that a test can stop a run in milliseconds instead of waiting [`TIME_LIMIT`].
    pub(crate) time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            instructions: INSTRUCTION_LIMIT,
            memory: MEMORY_LIMIT,
            time: TIME_LIMIT,
        }
    }
}

/// The limit that stopped a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Instructions,
    Time,
}

impl Stop {
    fn message(self, limits: Limits) -> String {
        match self {
            Self::Instructions => format!(
                "stopped by the instruction limit of {} instructions",
                limits.instructions
            ),
            Self::Time => format!(
                "stopped by the time limit of {} s",
                limits.time.as_secs_f64()
            ),
        }
    }
}

/// What a run has used of its budget. Shared with the hook and with `print`, which only see `&Lua`.
#[derive(Debug)]
struct Budget {
    executed: Cell<u64>,
    /// When the run began: the time limit counts from here.
    started: Cell<Instant>,
    /// The limit the run ran into. It stays set, so that `pcall` can tell a caught limit from a caught error.
    stopped: Cell<Option<Stop>>,
    /// The lines `print` and `warn` have written in the run.
    printed: Cell<usize>,
}

impl Budget {
    fn new() -> Self {
        Self {
            executed: Cell::new(0),
            started: Cell::new(Instant::now()),
            stopped: Cell::new(None),
            printed: Cell::new(0),
        }
    }

    /// Gives the next run the whole budget.
    fn restart(&self) {
        self.executed.set(0);
        self.started.set(Instant::now());
        self.stopped.set(None);
        self.printed.set(0);
    }

    /// Counts one tick of the hook, and returns the limit the run has now passed, if one.
    fn tick(&self, limits: Limits) -> Option<Stop> {
        let executed = self.executed.get().saturating_add(u64::from(HOOK_INTERVAL));
        self.executed.set(executed);
        let stop = if executed > limits.instructions {
            Stop::Instructions
        } else if self.started.get().elapsed() > limits.time {
            Stop::Time
        } else {
            return None;
        };
        self.stopped.set(Some(stop));
        Some(stop)
    }
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
        let budget = Rc::new(Budget::new());
        // Before the globals are pruned: the copy of a metatable needs the `next` that is dropped with them.
        metatables::guard(&lua)?;
        keep_safe_globals(&lua)?;
        let tostring = strings::restrict(&lua)?;
        tables::bound(&lua)?;
        output::route_to_log(&lua, &budget, &tostring)?;
        order_pairs(&lua)?;
        guard_limits(&lua, &budget, limits)?;
        lua.set_memory_limit(limits.memory)?;
        let hook_budget = Rc::clone(&budget);
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(HOOK_INTERVAL),
            move |_, _| match hook_budget.tick(limits) {
                Some(stop) => Err(Error::runtime(stop.message(limits))),
                None => Ok(VmState::Continue),
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
        self.budget.restart();
    }

    pub(crate) fn load<'a>(&self, name: &str, source: &'a str) -> Chunk<'a> {
        text_chunk(&self.lua, name, source)
    }

    /// The message of a failed run: Lua's own (`name:3: message`), or the limit that stopped it. At most [`MESSAGE_LIMIT`] bytes, because the config wrote it and reaches whoever reads it.
    pub(crate) fn explain(&self, error: &Error) -> String {
        let message = match self.budget.stopped.get() {
            Some(stop) => stop.message(self.limits),
            None => describe(error, self.limits.memory),
        };
        cut(message, MESSAGE_LIMIT)
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

/// An error of the native `name` that a wrapper called, worded the way Lua words it for a native called from Lua: with the position of the Lua call that reached it, the name of the function and no traceback. The original is called from a callback here, so it has no Lua caller to name and no name of its own; an error from Lua code inside a `__tostring` or a comparator already has its position.
pub(super) fn located(lua: &Lua, name: &str, error: Error) -> Error {
    match error {
        Error::RuntimeError(message) if !starts_with_position(&message) => {
            raise(lua, named(without_traceback(&message), name))
        }
        other => other,
    }
}

/// `message` with the `'?'` that Lua puts in `bad argument #2 to '?'` when it does not know the function replaced by `name`.
fn named(message: &str, name: &str) -> String {
    match message
        .strip_prefix("bad argument #")
        .and_then(|rest| rest.split_once(" to '?'"))
    {
        Some((number, rest)) => format!("bad argument #{number} to '{name}'{rest}"),
        None => message.to_owned(),
    }
}

/// Whether `message` begins the way Lua begins an error raised in a chunk: `name:3: `.
fn starts_with_position(message: &str) -> bool {
    message
        .split_once(": ")
        .and_then(|(head, _)| head.rsplit_once(':'))
        .is_some_and(|(_, line)| !line.is_empty() && line.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The integer a native would read `value` as: an integer, a float that holds one, or a string that spells one. `None` for anything else, where the native raises its own error.
fn integer(lua: &Lua, value: &Value) -> Option<i64> {
    lua.coerce_integer(value.clone()).ok().flatten()
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

fn guard_limits(lua: &Lua, budget: &Rc<Budget>, limits: Limits) -> mlua::Result<()> {
    let budget = Rc::clone(budget);
    let check = lua.create_function(move |_, results: Variadic<Value>| {
        if let Some(stop) = budget.stopped.get() {
            return Err(Error::runtime(stop.message(limits)));
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
/// `next` is not kept: it would show that order to a script that calls it by hand.
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
    use proptest::prelude::*;

    use super::*;
    use crate::testlog::capture;

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
            ..Limits::default()
        })
        .unwrap()
    }

    /// A sandbox that stops a run after `time`, not after the two seconds of the spec.
    fn hurried(time: Duration) -> Sandbox {
        Sandbox::with_limits(Limits {
            time,
            ..Limits::default()
        })
        .unwrap()
    }

    /// What `run` writes to the log under the target of the config's output, one entry per line, without the level.
    fn logged(run: impl FnOnce()) -> Vec<String> {
        capture(run)
            .iter()
            .filter_map(|line| line.split_once("gsttt_config::lua: "))
            .map(|(_, message)| message.to_owned())
            .collect()
    }

    /// The results of a call, shown the way `tostring` shows them and joined by commas, so that a test sees how many there are.
    const SHOW: &str = r##"
        local function show(...)
          local shown = {}
          for index = 1, select("#", ...) do shown[index] = tostring((select(index, ...))) end
          return table.concat(shown, ",")
        end
    "##;

    #[test]
    fn a_message_with_a_position_is_told_from_one_without() {
        for message in ["t.lua:5: boom", "[string \"x\"]:12: boom: more", "a.b:1: x"] {
            assert!(starts_with_position(message), "{message}");
        }
        for message in [
            "bad argument #2 to 'format' (number expected, got string)",
            "invalid conversion '%y' to 'format'",
            "t.lua: no line",
            "t.lua:x: no number",
            "",
        ] {
            assert!(!starts_with_position(message), "{message}");
        }
    }

    #[test]
    fn an_argument_error_names_the_function_that_was_called() {
        assert_eq!(
            named("bad argument #2 to '?' (position out of bounds)", "insert"),
            "bad argument #2 to 'insert' (position out of bounds)"
        );
        for message in [
            "bad argument #2 to 'insert' (position out of bounds)",
            "invalid value (nil) at index 1 in table for 'concat'",
            "the name is to '?' here",
        ] {
            assert_eq!(named(message, "insert"), message);
        }
    }

    #[test]
    fn limits_default_to_the_spec() {
        assert_eq!(
            Limits::default(),
            Limits {
                instructions: 100_000_000,
                memory: 64 * 1024 * 1024,
                time: Duration::from_secs(2)
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
            "next",
            "math.random",
            "math.randomseed",
            "string.dump",
            "string.match",
            "string.gmatch",
            "string.gsub",
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
        let log = capture(|| {
            let sandbox = Sandbox::new().unwrap();
            eval(&sandbox, r#"print("hello", 1, true) warn("careful")"#).unwrap();
        });
        assert_eq!(
            log,
            [
                " INFO gsttt_config::lua: hello\t1\ttrue",
                " WARN gsttt_config::lua: careful"
            ]
        );
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

    #[test]
    fn the_pattern_functions_are_gone() {
        let sandbox = Sandbox::new().unwrap();
        // A method goes through the same table as the function.
        let message = eval(&sandbox, r#"return ("name"):match("%a+")"#).unwrap_err();
        assert!(
            message.starts_with("t.lua:1: ") && message.contains("match"),
            "{message}"
        );
        for name in ["gmatch", "gsub"] {
            let source = format!(r#"return ("name"):{name}("a", "b")"#);
            let message = eval(&sandbox, &source).unwrap_err();
            assert!(message.contains(name), "{message}");
        }
    }

    #[test]
    fn find_reads_what_it_is_given_as_text() {
        let sandbox = Sandbox::new().unwrap();
        let cases = [
            // The dot of a pattern is any character; here it is a dot.
            (r#"("a.c"):find(".")"#, "2,2"),
            (r#"("abc"):find(".")"#, "nil"),
            // The fourth argument asked for a plain search, and it asks for nothing now.
            (r#"("a.c"):find(".", 1, false)"#, "2,2"),
            (r#"("a.c"):find(".", 1, true)"#, "2,2"),
            (r#"("a%b"):find("%")"#, "2,2"),
            (r#"("aaa"):find("a*")"#, "nil"),
            // A pattern that does not even parse.
            (r#"("a(b"):find("(b")"#, "2,3"),
            (r#"("abc"):find("b", -2)"#, "2,2"),
            (r#"("abc"):find("")"#, "1,0"),
            (r#"("abc"):find("", 4)"#, "4,3"),
            (r#"("abc"):find("", 5)"#, "nil"),
            ("string.find(\"a1b\", 1)", "2,2"),
        ];
        for (call, expected) in cases {
            let source = format!("{SHOW} return show({call})");
            assert_eq!(eval(&sandbox, &source).unwrap(), expected, "{call}");
        }
        for (call, expected) in [
            (
                "string.find(nil, 'x')",
                "#1 to 'find' (string expected, got nil)",
            ),
            (
                "string.find('x', {})",
                "#2 to 'find' (string expected, got table)",
            ),
            (
                "string.find('x', 'x', 'y')",
                "#3 to 'find' (integer expected, got string)",
            ),
        ] {
            let message = eval(&sandbox, &format!("return {call}")).unwrap_err();
            assert!(
                message.starts_with("t.lua:1: bad argument ") && message.contains(expected),
                "{call}: {message}"
            );
        }
    }

    #[test]
    fn find_stays_fast_when_the_text_almost_matches_at_every_start() {
        // The search of Lua compares the whole text at each of the half million starts and fails at its last byte each time: minutes of native code that no limit interrupts. This one reads the subject once.
        let sandbox = Sandbox::new().unwrap();
        let started = Instant::now();
        let source = r#"
            local subject = string.rep("a", 1048576)
            local text = string.rep("a", 524288) .. "b"
            return tostring(subject:find(text))
        "#;
        assert_eq!(eval(&sandbox, source).unwrap(), "nil");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_table_or_a_function_is_shown_by_its_type() {
        let sandbox = Sandbox::new().unwrap();
        let source = format!(
            r#"{SHOW}
            local custom = setmetatable({{}}, {{ __tostring = function() return "custom" end }})
            return show(tostring({{}}), tostring(print), tostring(custom), tostring(1.0), tostring(nil), tostring("x"), tostring(12),
                        string.format("%s|%s|%s|%s", {{}}, print, custom, 1.5))"#
        );
        assert_eq!(
            eval(&sandbox, &source).unwrap(),
            "table,function,custom,1.0,nil,x,12,table|function|custom|1.5"
        );
    }

    #[test]
    fn format_refuses_the_pointer_conversion_and_keeps_the_others() {
        let sandbox = Sandbox::new().unwrap();
        for source in [
            r#"return string.format("%p", {})"#,
            r#"return ("x %5p"):format(print)"#,
        ] {
            let message = eval(&sandbox, source).unwrap_err();
            assert!(
                message.starts_with("t.lua:1: ") && message.contains("%p"),
                "{source}: {message}"
            );
        }
        assert_eq!(
            eval(
                &sandbox,
                r#"return ("%5.2f|%d|%s|%q|%%p"):format(3.14159, 7, "s", "q")"#
            )
            .unwrap(),
            " 3.14|7|s|\"q\"|%p"
        );
        // The mistakes the original reports keep naming the line of the call.
        let message = eval(&sandbox, "local x = 1\nreturn string.format('%d', 'x')").unwrap_err();
        assert!(message.starts_with("t.lua:2: bad argument #2"), "{message}");
        let message = eval(&sandbox, "return ('%y'):format(1)").unwrap_err();
        assert!(
            message.starts_with("t.lua:1: invalid conversion"),
            "{message}"
        );
    }

    #[gsttt_scenario::scenario("config", "A loop of cheap calls stops at the time limit")]
    #[test]
    fn a_loop_of_cheap_calls_stops_at_the_time_limit() {
        // A few instructions and one search over a megabyte per pass: the hundred million instructions of the limit would take over an hour of them.
        let sandbox = hurried(Duration::from_millis(200));
        let search = r#"local subject = string.rep("a", 1 << 20) "#;
        let loops = [
            format!("{search} while true do subject:find('b') end"),
            format!(
                "{search} while true do pcall(function() for _ = 1, 1e9 do subject:find('b') end end) end"
            ),
        ];
        for source in loops {
            sandbox.reset_budget();
            let started = Instant::now();
            let message = eval(&sandbox, &source).unwrap_err();
            assert!(message.contains("time limit of 0.2 s"), "{message}");
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "{:?}",
                started.elapsed()
            );
        }
        sandbox.reset_budget();
        assert_eq!(eval(&sandbox, "return 1 + 1").unwrap(), "2");
    }

    #[gsttt_scenario::scenario("config", "A printed line is plain text")]
    #[test]
    fn a_printed_line_is_plain_text_of_at_most_four_kibibytes() {
        let lines = logged(|| {
            let sandbox = Sandbox::new().unwrap();
            eval(
                &sandbox,
                r#"
                print("a\27[31mred\nb\0c\td")
                print(string.rep("x", 10000))
                print({}, print, nil, 1.0)
                warn("w\r")
                "#,
            )
            .unwrap();
        });
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert_eq!(lines[0], "a[31mredbc\td");
        assert_eq!(lines[1].len(), 4096);
        assert!(lines[1].starts_with("xxx") && lines[1].ends_with("x…"));
        assert_eq!(lines[2], "table\tfunction\tnil\t1.0");
        assert_eq!(lines[3], "w");
    }

    #[gsttt_scenario::scenario("config", "Output is bounded")]
    #[test]
    fn a_run_writes_a_thousand_lines_and_then_one_that_says_the_rest_is_dropped() {
        let lines = logged(|| {
            let sandbox = Sandbox::new().unwrap();
            // `print` and `warn` draw on the same thousand.
            eval(
                &sandbox,
                r#"for index = 1, 5000 do if index % 2 == 0 then print(index) else warn(index) end end print("late")"#,
            )
            .unwrap();
            // The next run on the same state starts again.
            sandbox.reset_budget();
            eval(&sandbox, r#"print("again")"#).unwrap();
        });
        assert_eq!(lines.len(), 1002, "{:?}", lines.last());
        assert_eq!(lines[0], "1");
        assert_eq!(lines[999], "1000");
        assert!(lines[1000].contains("dropped"), "{}", lines[1000]);
        assert_eq!(lines[1001], "again");
    }

    #[test]
    fn a_long_error_message_is_cut() {
        let sandbox = Sandbox::new().unwrap();
        let message = eval(&sandbox, r#"error(string.rep("x", 5000))"#).unwrap_err();
        assert_eq!(message.len(), 1024);
        assert!(message.starts_with("t.lua:1: xxx") && message.ends_with("x…"));
        assert_eq!(
            eval(&sandbox, r#"error("short")"#).unwrap_err(),
            "t.lua:1: short"
        );
    }

    /// A table whose length is `2^40` by its border alone: forty-one keys, each twice the one before, all in the hash part of the table, and no `__len`.
    fn sparse_table() -> String {
        let keys: Vec<String> = (0..=40)
            .map(|power| format!("[{}] = true", 1u64 << power))
            .collect();
        format!("local sparse = {{ {} }}", keys.join(", "))
    }

    /// A table that answers three the first time it is asked its length and a trillion every time after.
    const STATEFUL: &str = r"
        local asked = 0
        local stateful = setmetatable({}, { __len = function() asked = asked + 1 return asked == 1 and 3 or 2 ^ 40 end })
    ";

    /// Natives as `__index` and `__newindex` make a table without a body, whose every read and write is a call that runs no Lua instruction.
    const NO_BODY: &str = "{ __index = type, __newindex = rawlen }";

    #[test]
    fn a_native_loop_that_the_script_sizes_is_refused_before_it_starts() {
        let sandbox = Sandbox::new().unwrap();
        let sparse = sparse_table();
        let range = "the range holds more than 1000000 elements";
        let long = "the table is longer than 1000000 elements";
        let length = "a table with a `__len` metamethod is not accepted";
        let cases = [
            ("table.move({}, 1, 2^40, 2)".to_owned(), range),
            (r#"table.move({}, "1", "1e12", 2)"#.to_owned(), range),
            ("table.move({}, -2^62, 2^62, 1)".to_owned(), range),
            (
                "table.move({}, math.mininteger, math.maxinteger, 1)".to_owned(),
                range,
            ),
            (
                format!("local t = setmetatable({{}}, {NO_BODY}) table.move(t, 1, 2^40, 1, t)"),
                range,
            ),
            (r#"table.concat({}, "", 1, 2^40)"#.to_owned(), range),
            (r#"table.concat({}, "", -2^40, 1)"#.to_owned(), range),
            // Without an end the range stops at the length of the table, and it still begins where the script says.
            (
                r#"table.concat(setmetatable({}, { __index = rawlen }), "", -2^40)"#.to_owned(),
                range,
            ),
            (
                r#"table.concat(setmetatable({}, { __index = rawlen }), "", math.mininteger)"#
                    .to_owned(),
                range,
            ),
            (format!("{sparse} table.insert(sparse, 1, 'x')"), long),
            (format!("{sparse} table.remove(sparse, 1)"), long),
            (format!("{sparse} table.sort(sparse)"), long),
            (format!("{sparse} table.concat(sparse)"), long),
            (
                format!("{sparse} table.sort(setmetatable(sparse, {NO_BODY}))"),
                long,
            ),
            (
                "table.sort(setmetatable({}, { __len = function() return 2 ^ 40 end }))".to_owned(),
                length,
            ),
            (format!("{STATEFUL} table.sort(stateful)"), length),
            (format!("{STATEFUL} table.insert(stateful, 1, 'x')"), length),
            (format!("{STATEFUL} table.remove(stateful, 1)"), length),
            (format!("{STATEFUL} table.concat(stateful)"), length),
        ];
        let started = Instant::now();
        for (source, expected) in &cases {
            let message = eval(&sandbox, source).expect_err(source);
            assert!(
                message.starts_with("t.lua:") && message.contains(expected),
                "{source}: {message}"
            );
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_table_or_a_range_of_exactly_the_budget_is_still_worked_on() {
        let sandbox = Sandbox::new().unwrap();
        let inserted = |length: u32| {
            format!(
                "local list = {{}} for index = 1, {length} do list[index] = index end table.insert(list, 1, 0) return #list"
            )
        };
        assert_eq!(eval(&sandbox, &inserted(1_000_000)).unwrap(), "1000001");
        let message = eval(&sandbox, &inserted(1_000_001)).unwrap_err();
        assert!(
            message.contains("longer than 1000000 elements"),
            "{message}"
        );
        assert_eq!(
            eval(&sandbox, "return #table.move({}, 1, 1000000, 2)").unwrap(),
            "0"
        );
        let message = eval(&sandbox, "table.move({}, 1, 1000001, 2)").unwrap_err();
        assert!(message.contains("more than 1000000 elements"), "{message}");
    }

    #[test]
    fn a_table_with_a_length_metamethod_is_left_alone_by_the_calls_that_do_not_walk_it() {
        let sandbox = Sandbox::new().unwrap();
        // `insert` with two arguments and `remove` without a position take one step, whatever the table answers.
        let source = format!(
            r#"{SHOW}
            local list = setmetatable({{}}, {{ __len = function() return 2 ^ 40 end }})
            table.insert(list, "x")
            return show(rawget(list, 2 ^ 40 + 1), table.remove(list))"#
        );
        assert_eq!(eval(&sandbox, &source).unwrap(), "x,nil");
    }

    #[test]
    fn repeating_nothing_returns_at_once_and_every_other_repeat_is_the_original() {
        let sandbox = Sandbox::new().unwrap();
        let started = Instant::now();
        let cases = [
            (r#"string.rep("", 2^62)"#, ""),
            (r#"string.rep("", 2^62, "")"#, ""),
            (r#"string.rep("", 2^62, nil)"#, ""),
            (r#"string.rep("", "4611686018427387904")"#, ""),
            (r#"(""):rep(math.maxinteger)"#, ""),
            (r#"string.rep("", 0)"#, ""),
            (r#"string.rep("ab", 3, ",")"#, "ab,ab,ab"),
            (r#"string.rep("", 3, ",")"#, ",,"),
            (r#"string.rep("ab", 0)"#, ""),
            ("string.rep(12, 2)", "1212"),
        ];
        for (call, expected) in cases {
            let source = format!("{SHOW} return show({call})");
            assert_eq!(eval(&sandbox, &source).unwrap(), expected, "{call}");
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        for (call, expected) in [
            (
                r#"string.rep("", {})"#,
                "#2 to 'rep' (number expected, got table)",
            ),
            (
                r#"string.rep("", 2.5)"#,
                "#2 to 'rep' (number has no integer representation)",
            ),
            (
                "string.rep({}, 2^62)",
                "#1 to 'rep' (string expected, got table)",
            ),
            (r#"string.rep("x", 2^62)"#, "resulting string too large"),
        ] {
            let message = eval(&sandbox, &format!("return {call}")).unwrap_err();
            assert!(
                message.starts_with("t.lua:1: ") && message.contains(expected),
                "{call}: {message}"
            );
        }
    }

    #[test]
    fn a_wrapped_call_keeps_its_results_its_errors_and_their_positions() {
        let sandbox = Sandbox::new().unwrap();
        let source = format!(
            r##"{SHOW}
            local list = {{ 3, 1, 2 }}
            return show(select("#", table.sort(list)), select("#", table.insert(list, 9)), table.remove(list, 1))"##
        );
        assert_eq!(eval(&sandbox, &source).unwrap(), "0,0,1");
        let message = eval(&sandbox, "table.insert({}, 5, 1)").unwrap_err();
        assert_eq!(
            message,
            "t.lua:1: bad argument #2 to 'insert' (position out of bounds)"
        );
        // Called by `pcall`, the original has no Lua caller to name, and neither does the wrapper. The error is an object, as every error that begins in Rust is.
        let source = "return tostring(select(2, pcall(table.insert, {}, 5, 1)))";
        let message = eval(&sandbox, source).unwrap();
        assert!(
            message.starts_with(
                "runtime error: bad argument #2 to 'insert' (position out of bounds)\n"
            ),
            "{message}"
        );
    }

    #[test]
    fn a_limit_reached_inside_a_wrapped_call_still_ends_the_run() {
        let sandbox = small(1_000_000, 64);
        let forever =
            "pcall(table.sort, { 3, 2, 1 }, function() while true do end end) return 'went on'";
        let message = eval(&sandbox, forever).unwrap_err();
        assert!(message.contains("instruction limit"), "{message}");
        let sandbox = hurried(Duration::from_millis(100));
        let message =
            eval(&sandbox, "while true do pcall(table.insert, {}, 1, 1) end").unwrap_err();
        assert!(message.contains("time limit"), "{message}");
    }

    #[test]
    fn a_weak_table_cannot_stall_the_collector() {
        let sandbox = Sandbox::new().unwrap();
        for mode in ["k", "v", "kv"] {
            let source =
                format!(r#"local weak = setmetatable({{}}, {{ __mode = "{mode}" }}) return weak"#);
            let message = eval(&sandbox, &source).unwrap_err();
            assert!(
                message.starts_with("t.lua:1: ") && message.contains("`__mode`"),
                "{message}"
            );
        }
        // A field written with `rawset` is a field all the same.
        let source = r#"local weak = setmetatable({}, rawset({}, "__mode", "k")) return weak"#;
        let message = eval(&sandbox, source).unwrap_err();
        assert!(message.contains("`__mode`"), "{message}");
        assert_eq!(
            eval(
                &sandbox,
                "return type(setmetatable({}, { __index = {}, __call = print }))"
            )
            .unwrap(),
            "table"
        );
    }

    /// `show` for results that may hold an error. An error that began in Rust is an object whose text begins with `runtime error: ` and ends in a traceback, and a table is told by its type because its text holds an address.
    const SHOW_OUTCOME: &str = r##"
        local function show(...)
          local shown = {}
          for index = 1, select("#", ...) do
            local value = (select(index, ...))
            local text = type(value) == "table" and "table" or tostring(value)
            if text:sub(1, 15) == "runtime error: " then text = text:sub(16) end
            local traceback = text:find("\nstack traceback:", 1, true)
            if traceback then text = text:sub(1, traceback - 1) end
            shown[#shown + 1] = text
          end
          return table.concat(shown, ",")
        end
    "##;

    /// Calls of `setmetatable` and `getmetatable` that the state answers as Lua does, one line of output for each. What differs is a metatable that is written into after the call, and one with `__gc` or `__mode`; those have their own tests.
    const METATABLE_CASES: &str = r##"
        local out = {}
        local function run(name, call) out[#out + 1] = name .. ":" .. show(pcall(call)) end
        run("no arguments", function() return setmetatable() end)
        run("nil target", function() return setmetatable(nil, {}) end)
        run("number target", function() return setmetatable(1, {}) end)
        run("string target", function() return setmetatable("text", {}) end)
        run("boolean target", function() return setmetatable(true, {}) end)
        run("function target", function() return setmetatable(print, {}) end)
        run("no metatable", function() return setmetatable({}) end)
        run("number metatable", function() return setmetatable({}, 1) end)
        run("string metatable", function() return setmetatable({}, "text") end)
        run("true metatable", function() return setmetatable({}, true) end)
        run("false metatable", function() return setmetatable({}, false) end)
        run("function metatable", function() return setmetatable({}, print) end)
        run("extra arguments", function() return type(setmetatable({}, nil, 1, 2)), type(setmetatable({}, {}, "extra")) end)
        run("protected by a string", function() local t = setmetatable({}, { __metatable = "sealed" }) return setmetatable(t, {}) end)
        run("protected by false", function() local t = setmetatable({}, { __metatable = false }) return setmetatable(t, {}) end)
        run("protected by zero", function() local t = setmetatable({}, { __metatable = 0 }) return setmetatable(t, nil) end)
        run("protected by a table", function() local t = setmetatable({}, { __metatable = {} }) return setmetatable(t, {}) end)
        run("protected by itself", function() local mt = { __metatable = true } local t = setmetatable({}, mt) return setmetatable(t, mt) end)
        run("not protected", function() local t = setmetatable({}, {}) return type(setmetatable(t, {})), type(setmetatable(t, nil)), type(setmetatable(t, {})) end)
        run("what getmetatable gives", function()
          local mt, hidden = {}, {}
          return getmetatable(setmetatable({}, mt)) == mt,
            getmetatable(setmetatable({}, { __metatable = "sealed" })),
            getmetatable(setmetatable({}, { __metatable = false })),
            getmetatable(setmetatable({}, { __metatable = hidden })) == hidden,
            getmetatable({}), type(getmetatable("")), getmetatable(1)
        end)
        run("what setmetatable gives", function()
          local t = {}
          return setmetatable(t, {}) == t, setmetatable(t, nil) == t, select("#", setmetatable({}, {}))
        end)
        run("same class", function()
          local Class = {}
          local a, b = setmetatable({}, Class), setmetatable({}, Class)
          return getmetatable(a) == getmetatable(b), rawequal(getmetatable(a), Class)
        end)
        run("replace and remove", function()
          local t = setmetatable({}, { __index = { v = 1 } })
          local first = t.v
          setmetatable(t, { __index = { v = 2 } })
          local second = t.v
          setmetatable(t, nil)
          return first, second, t.v
        end)
        run("class", function()
          local Class = {}
          Class.__index = Class
          function Class.get(self) return self.value end
          local o = setmetatable({ value = 5 }, Class)
          function Class.late() return "late" end
          return o:get(), o.late(), o.nothing
        end)
        run("inheritance", function()
          local Base = {}
          Base.__index = Base
          function Base.hello() return "base" end
          local Derived = setmetatable({}, Base)
          Derived.__index = Derived
          local o = setmetatable({}, Derived)
          function Base.later() return "later" end
          return o.hello(), o.later(), getmetatable(o) == Derived, getmetatable(Derived) == Base
        end)
        run("shared tables", function()
          local mt = { __index = { v = 1 } }
          local a, b = setmetatable({}, mt), setmetatable({}, mt)
          mt.__index.v = 2
          return a.v, b.v
        end)
        run("operators", function()
          local mt = {
            __add = function() return "add" end,
            __sub = function() return "sub" end,
            __concat = function() return "concat" end,
            __len = function() return 7 end,
            __unm = function() return "unm" end,
            __eq = function() return true end,
            __lt = function() return true end,
            __le = function() return false end,
            __call = function(self, x) return "call " .. tostring(x) end,
            __tostring = function() return "custom" end,
            __index = function(t, k) return "index " .. k end,
            __newindex = function(t, k, v) rawset(t, k, v * 2) end,
          }
          local a, b = setmetatable({}, mt), setmetatable({}, mt)
          a.stored = 4
          return a + b, a - 1, a .. "x", "x" .. a, #a, -a, a == b, a < b, a <= b, a(5), tostring(a), a.missing, a.stored
        end)
        run("close", function()
          local closed = false
          do local c <close> = setmetatable({}, { __close = function() closed = true end }) end
          return closed
        end)
        return table.concat(out, "\n")
    "##;

    #[test]
    fn setmetatable_and_getmetatable_agree_with_the_originals() {
        let source = [SHOW_OUTCOME, METATABLE_CASES].concat();
        let original = Lua::new();
        let expected = original
            .load(&source)
            .set_name("@t.lua")
            .eval::<String>()
            .unwrap();
        let sandbox = Sandbox::new().unwrap();
        assert_eq!(eval(&sandbox, &source).unwrap(), expected);
    }

    #[test]
    fn copying_a_metatable_is_counted_by_the_instruction_limit() {
        // The copy is a loop in Lua, so a metatable of twenty thousand fields costs twenty thousand loops a call. A native copy would cost one instruction, and a thousand calls would run for seconds inside one look of the hook.
        let sandbox = small(2_000_000, 64);
        let started = Instant::now();
        let message = eval(
            &sandbox,
            r#"
            local meta = {}
            for i = 1, 20000 do meta["field" .. i] = i end
            for _ = 1, 1000 do setmetatable({}, meta) end
            "#,
        )
        .unwrap_err();
        assert!(message.contains("instruction limit"), "{message}");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn no_table_of_the_state_has_a_metatable_the_config_can_reach() {
        // A walk over everything the config can name. The metatable of strings is out of the walk: no table has it, so Lua never reads `__mode` or `__gc` from it.
        let sandbox = Sandbox::new().unwrap();
        let walked = eval(
            &sandbox,
            r#"
            local names, queue, found = { [_G] = "_G" }, { _G }, {}
            local index = 1
            while queue[index] do
              local current = queue[index]
              index = index + 1
              if type(getmetatable(current)) == "table" then found[#found + 1] = names[current] end
              for key, value in pairs(current) do
                if type(value) == "table" and not names[value] then
                  names[value] = names[current] .. "." .. tostring(key)
                  queue[#queue + 1] = value
                end
              end
            end
            return #queue .. "|" .. table.concat(found, ",")
            "#,
        )
        .unwrap();
        let (visited, reachable) = walked.split_once('|').unwrap();
        assert!(visited.parse::<usize>().unwrap() >= 5, "{walked}");
        assert_eq!(reachable, "", "{walked}");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Inside the budget the bounded calls are the originals: the same results, the same number of results, the same errors and the same state of the list afterwards.
        #[test]
        fn the_bounded_library_calls_agree_with_the_originals(
            items in prop::collection::vec(-9i64..9, 0..8),
            position in -2i64..11,
            from in -2i64..10,
            to in -2i64..10,
            target in -2i64..10,
            count in -2i64..5,
        ) {
            let list = items.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            let source = format!(
                r#"
                {SHOW_OUTCOME}
                local out = {{}}
                local function run(name, call)
                  local subject = {{ {list} }}
                  out[#out + 1] = name .. ":" .. show(pcall(call, subject)) .. "|" .. show(table.unpack(subject))
                end
                run("insert", function(list) return (table.insert(list, {position}, 7)) end)
                run("append", function(list) return (table.insert(list, 7)) end)
                run("remove", function(list) return (table.remove(list, {position})) end)
                run("pop", function(list) return (table.remove(list)) end)
                run("sort", function(list) return (table.sort(list)) end)
                run("sort_down", function(list) return (table.sort(list, function(a, b) return a > b end)) end)
                run("move", function(list) return (table.move(list, {from}, {to}, {target})) end)
                run("concat", function(list) return (table.concat(list, ",", {from}, {to})) end)
                run("concat_from", function(list) return (table.concat(list, ",", {from})) end)
                run("concat_all", function(list) return (table.concat(list, ",")) end)
                run("rep", function() return (string.rep("ab", {count}, ",")) end)
                run("rep_nothing", function() return (string.rep("", {count}, "")) end)
                return table.concat(out, ";")"#
            );
            let original = Lua::new();
            let expected = original.load(&source).set_name("@t.lua").eval::<String>().unwrap();
            let sandbox = Sandbox::new().unwrap();
            prop_assert_eq!(eval(&sandbox, &source).unwrap(), expected);
        }
    }
}
