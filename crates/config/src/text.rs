//! Text that came from outside gsttt (what a connector writes to its standard error, a key or a value of a file of the config that an error quotes) made fit for a log line or a message.
//!
//! A reader of the log, or a terminal, takes a line for one line of plain text. A control character breaks that: an escape sequence recolours or rewrites what is on the screen, and a newline starts a line the writer forged. A line that runs to megabytes buries the rest of the log.

/// What a cut puts in place of the text it removes.
const CUT_MARK: &str = "…";

/// The longest line of the log, in bytes: what a process writes to its standard error, and a message that quotes a file of the config.
pub(crate) const LOG_LINE_LIMIT: usize = 4 * 1024;

/// `text` as one line of the log: control characters other than tab are dropped, and what is left is cut to [`LOG_LINE_LIMIT`] bytes.
pub(crate) fn log_line(text: &str) -> String {
    // One byte more than the limit is enough to know the text is cut, so a text of any length is read only as far as that.
    let line: String = text
        .chars()
        .filter(|character| *character == '\t' || !character.is_control())
        .take(LOG_LINE_LIMIT + 1)
        .collect();
    cut(line, LOG_LINE_LIMIT)
}

/// `text` cut to at most `limit` bytes, never inside a character. A cut text ends in `…`, so a reader sees that there was more, unless `limit` is too small to hold the mark: it is then cut at the limit without it, because a result longer than its limit is worse than one that does not say it was cut.
pub(crate) fn cut(mut text: String, limit: usize) -> String {
    if text.len() <= limit {
        return text;
    }
    let mark = if limit >= CUT_MARK.len() {
        CUT_MARK
    } else {
        ""
    };
    let mut end = limit.saturating_sub(mark.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(mark);
    text
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_line_loses_its_control_characters_but_keeps_its_tabs() {
        assert_eq!(log_line("a\u{1b}[31mb\r\nc\td\u{0}\u{85}e"), "a[31mbc\tde");
        assert_eq!(log_line("plain text, ünïcödé ✓"), "plain text, ünïcödé ✓");
    }

    #[test]
    fn a_line_is_cut_to_the_limit_and_says_so() {
        let long = "x".repeat(10_000);
        let line = log_line(&long);
        assert_eq!(line.len(), LOG_LINE_LIMIT);
        assert!(line.ends_with('…'));
        let exact = "y".repeat(LOG_LINE_LIMIT);
        assert_eq!(log_line(&exact), exact);
        let over = "y".repeat(LOG_LINE_LIMIT + 1);
        assert!(log_line(&over).ends_with('…'));
    }

    #[test]
    fn a_cut_never_lands_inside_a_character() {
        // Three bytes each, so every cut point of the limit falls inside one for two of the three offsets.
        let text = "✓".repeat(2_000);
        for limit in 3..40 {
            let shortened = cut(text.clone(), limit);
            assert!(shortened.len() <= limit, "{limit}: {}", shortened.len());
            assert!(shortened.ends_with('…'), "{limit}");
        }
        assert_eq!(cut("short".to_owned(), 5), "short");
    }

    #[test]
    fn a_limit_too_small_for_the_mark_cuts_without_it() {
        assert_eq!(cut("abcdef".to_owned(), 2), "ab");
        assert_eq!(cut("✓✓".to_owned(), 2), "");
        assert_eq!(cut("abc".to_owned(), 0), "");
    }

    proptest! {
        #[test]
        fn a_cut_text_is_never_longer_than_its_limit(text in "\\PC{0,40}", limit in 0usize..48) {
            let cut_text = cut(text.clone(), limit);
            prop_assert!(cut_text.len() <= limit, "{} > {limit}: {cut_text:?}", cut_text.len());
            if text.len() <= limit {
                prop_assert_eq!(&cut_text, &text);
            } else {
                // The mark is there exactly when it fits, and what precedes it is the start of the text.
                prop_assert_eq!(cut_text.ends_with(CUT_MARK), limit >= CUT_MARK.len());
                let kept = cut_text.strip_suffix(CUT_MARK).unwrap_or(&cut_text);
                prop_assert!(text.starts_with(kept), "{kept:?} is not the start of {text:?}");
            }
        }
    }

    #[test]
    fn a_line_made_only_of_control_characters_is_empty() {
        assert_eq!(log_line(&"\u{7}\n".repeat(100_000)), "");
    }
}
