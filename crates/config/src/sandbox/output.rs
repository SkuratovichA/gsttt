//! `print` and `warn` of a config: they write to the log, a bounded number of lines a run, each of them one plain line.
//!
//! A config that prints in a loop would write millions of lines, and what it prints is its own text, so a line is stripped of the characters that move a terminal's cursor or start a line the writer forged, and cut (`crate::text::log_line`).

use std::cmp::Ordering;
use std::rc::Rc;

use mlua::{Function, Lua, LuaString, Value, Variadic};

use super::Budget;
use crate::text::{LOG_LINE_LIMIT, log_line};

/// Lines `print` and `warn` write in one run. The lines after them are dropped, and one line says so.
const MAX_LINES: usize = 1_000;

/// The target of the lines in the log, so that a reader can filter on it.
const TARGET: &str = "gsttt_config::lua";

/// Replaces `print` and `warn`. `tostring` is how a value is shown, the one the config itself calls.
pub(super) fn route_to_log(
    lua: &Lua,
    budget: &Rc<Budget>,
    tostring: &Function,
) -> mlua::Result<()> {
    let print = writer(lua, budget, tostring, |line: &str| {
        tracing::info!(target: TARGET, "{line}");
    })?;
    let warn = writer(lua, budget, tostring, |line: &str| {
        tracing::warn!(target: TARGET, "{line}");
    })?;
    let globals = lua.globals();
    globals.raw_set("print", print)?;
    globals.raw_set("warn", warn)
}

/// A function that counts a call against the line budget of the run and, while there is budget, hands its words to `emit` as one line.
fn writer(
    lua: &Lua,
    budget: &Rc<Budget>,
    tostring: &Function,
    emit: fn(&str),
) -> mlua::Result<Function> {
    let budget = Rc::clone(budget);
    let tostring = tostring.clone();
    lua.create_function(move |_, words: Variadic<Value>| {
        let written = budget.printed.get();
        budget.printed.set(written.saturating_add(1));
        match written.cmp(&MAX_LINES) {
            Ordering::Less => emit(&log_line(&tab_separated(&tostring, &words)?)),
            Ordering::Equal => tracing::warn!(
                target: TARGET,
                "the rest of the output of this run is dropped: a run writes at most {MAX_LINES} lines"
            ),
            Ordering::Greater => {}
        }
        Ok(())
    })
}

/// The words joined by tabs, as `print` writes them, each shown the way `tostring` shows it. A word longer than a line is read only as far as a line goes, because the rest is cut anyway.
fn tab_separated(tostring: &Function, words: &[Value]) -> mlua::Result<String> {
    let mut line = String::new();
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            line.push('\t');
        }
        let text: LuaString = tostring.call(word.clone())?;
        let bytes = text.as_bytes();
        let head = bytes.get(..=LOG_LINE_LIMIT).unwrap_or(&bytes);
        line.push_str(&String::from_utf8_lossy(head));
        if line.len() > LOG_LINE_LIMIT {
            break;
        }
    }
    Ok(line)
}
