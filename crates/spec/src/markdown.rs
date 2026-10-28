//! The slice of Markdown that OpenSpec files use: lines, fenced code, headings and list items.

/// One line of a file and whether it sits inside a fenced code block.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line<'a> {
    /// Counted from 1.
    pub(crate) number: usize,
    /// Without the line ending.
    pub(crate) text: &'a str,
    /// Fence lines count as fenced, like in OpenSpec's code-fence mask.
    pub(crate) fenced: bool,
}

/// The lines of a file and the fence that nothing closes.
pub(crate) struct Lines<'a> {
    pub(crate) lines: Vec<Line<'a>>,
    /// The line that opens a fence that is still open at the end of the file, from which every
    /// line counts as fenced.
    pub(crate) unclosed: Option<usize>,
}

/// Splits `text` into lines and marks the fenced ones.
pub(crate) fn lines(text: &str) -> Lines<'_> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // The fence that is open, with the line that opened it.
    let mut open: Option<((char, usize), usize)> = None;
    let lines = text
        .lines()
        .enumerate()
        .map(|(index, text)| {
            let number = index + 1;
            let fenced = match open {
                None => {
                    open = fence(text).map(|opening| (opening, number));
                    open.is_some()
                }
                Some((opening, _)) => {
                    if closes(text, opening) {
                        open = None;
                    }
                    true
                }
            };
            Line {
                number,
                text,
                fenced,
            }
        })
        .collect();
    Lines {
        lines,
        unclosed: open.map(|(_, number)| number),
    }
}

/// The marker and length of the fence a line opens, if it opens one.
fn fence(text: &str) -> Option<(char, usize)> {
    let text = text.trim_start();
    let marker = text.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let length = text.chars().take_while(|&c| c == marker).count();
    (length >= 3).then_some((marker, length))
}

/// Whether a line closes the fence `opening`: the same marker, at least as long, and nothing else.
fn closes(text: &str, (marker, length): (char, usize)) -> bool {
    let text = text.trim();
    text.chars().all(|c| c == marker) && text.chars().count() >= length
}

/// Splits `## Title` into its level and its text; a heading starts in column 0 and needs whitespace after the `#` run.
pub(crate) fn heading(text: &str) -> Option<(usize, &str)> {
    let level = text.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = text.get(level..)?;
    rest.starts_with(char::is_whitespace)
        .then_some((level, rest.trim()))
}

/// The text after a case-insensitive `label` such as `Requirement:`, without a closing run of `#`.
pub(crate) fn after_label<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    let (head, tail) = text.split_at_checked(label.len())?;
    if !head.eq_ignore_ascii_case(label) {
        return None;
    }
    let value = strip_closing_hashes(tail);
    (!value.is_empty()).then_some(value)
}

/// Drops the optional closing run of `#` that Markdown allows after a space, so `C#` keeps its `#`.
fn strip_closing_hashes(text: &str) -> &str {
    let text = text.trim();
    let without = text.trim_end_matches('#');
    if without.len() < text.len() && without.ends_with([' ', '\t']) {
        without.trim_end()
    } else {
        text
    }
}

/// The requirement name in a bullet such as ``- `### Requirement: Name` ``, how `REMOVED` and `RENAMED` sections list them.
pub(crate) fn requirement_ref(text: &str) -> Option<&str> {
    let text = text
        .trim()
        .trim_start_matches(['-', '*', '+'])
        .trim()
        .trim_matches('`')
        .trim_start();
    after_label(text.strip_prefix("###")?.trim_start(), "Requirement:")
}

/// The keyword of a scenario step such as `- **WHEN** …`, `- **THEN** …` or `- **AND** …`.
pub(crate) fn step_keyword(text: &str) -> Option<&str> {
    let rest = text
        .trim_start()
        .strip_prefix(['-', '*', '+'])?
        .trim_start()
        .strip_prefix("**")?;
    let (word, _) = rest.split_once("**")?;
    Some(word.trim_end_matches(':').trim())
}
