//! The Lua state every config source runs in: `init.lua`, its local modules and the rule packs.
//!
//! The state has the pure libraries and the base functions that cannot reach outside it, a memory limit, an instruction limit, a time limit and a `pairs` with a fixed order, so the same files always evaluate to the same setup.
//!
//! The instruction limit counts Lua instructions, and a native call is one instruction however long it runs. Two things make that safe. The time limit is checked with the instruction count, so a loop of native calls that each cost a little still stops. And the one native call that could run for ever has no way to: the pattern matcher of `string` is not in the state (see `strings.rs`).

mod output;
mod strings;

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
        keep_safe_globals(&lua)?;
        let tostring = strings::restrict(&lua)?;
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
}
