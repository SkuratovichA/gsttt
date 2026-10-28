//! The checkboxes of a change's `tasks.md`, counted the way OpenSpec counts them.

use crate::markdown;

/// One checkbox line of `tasks.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The text after the checkbox.
    pub text: String,
    /// Whether the box holds `x` or `X`; any other mark, such as `~`, leaves the task open.
    pub done: bool,
    /// The line of the task, counted from 1.
    pub line: usize,
}

/// Reads the checkboxes of a `tasks.md`: nested ones count, and so do those in fenced code, because
/// `openspec list` counts them too and the gate must agree with it on whether a change is finished.
#[must_use]
pub fn parse_tasks(text: &str) -> Vec<Task> {
    markdown::lines(text)
        .lines
        .into_iter()
        .filter_map(|line| {
            let (done, text) = checkbox(line.text)?;
            Some(Task {
                text: text.to_owned(),
                done,
                line: line.number,
            })
        })
        .collect()
}

/// Splits `- [x] text` into whether the box is ticked and the text; `[x](link)` is not a checkbox.
fn checkbox(line: &str) -> Option<(bool, &str)> {
    let rest = list_item(line)?.trim_start().strip_prefix('[')?;
    let (inside, after) = rest.split_once(']')?;
    let mark = inside.trim();
    let link = after.starts_with(['(', '[']);
    let done = match mark.chars().count() {
        0 if inside.is_empty() && link => return None,
        0 => false,
        1 if !link => matches!(mark, "x" | "X"),
        _ => return None,
    };
    Some((done, after.trim()))
}

/// What follows the bullet (`-`, `*`, `+`) or number (`1.`, `1)`) that starts a list item.
fn list_item(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return Some(rest);
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if !(1..=9).contains(&digits) {
        return None;
    }
    line.get(digits..)?.strip_prefix(['.', ')'])
}
