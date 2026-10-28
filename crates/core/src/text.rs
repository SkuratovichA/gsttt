//! Text helpers shared by the connectors and by the words of answers: line numbers from byte offsets, counts,
//! names and times.

use crate::fact::Line;

/// Maps byte offsets to 1-based line numbers, for parsers that report byte spans.
#[derive(Debug, Clone)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = Vec::with_capacity(text.len() / 32 + 1);
        starts.push(0);
        starts.extend(
            memchr::memchr_iter(b'\n', text.as_bytes())
                .map(|newline| u32::try_from(newline + 1).unwrap_or(u32::MAX)),
        );
        Self { starts }
    }

    /// The line holding the byte at `offset`.
    pub fn line(&self, offset: u32) -> Line {
        let line = self.starts.partition_point(|&start| start <= offset);
        u32::try_from(line).unwrap_or(u32::MAX).max(1)
    }

    /// The line holding the byte at `offset`, for a parser that reports its spans as `usize`. An offset that does not fit a `u32` lies on the last line, as the starts of the lines saturate at `u32::MAX` too.
    pub fn line_at(&self, offset: usize) -> Line {
        self.line(u32::try_from(offset).unwrap_or(u32::MAX))
    }

    /// The 1-based column of the byte at `offset` on its line, counted in characters, so that it is the column an editor shows. `text` is the text the index was built from. An offset inside a character is the column of that character, and one past the end of the text is the column after the last character.
    pub fn column(&self, text: &str, offset: u32) -> u32 {
        let start = usize::try_from(self.line(offset))
            .ok()
            .and_then(|line| line.checked_sub(1))
            .and_then(|index| self.starts.get(index))
            .map_or(0, |&start| usize::try_from(start).unwrap_or(usize::MAX));
        let mut end = usize::try_from(offset).map_or(text.len(), |offset| offset.min(text.len()));
        // Offset 0 is always a boundary, so this ends.
        while !text.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        let before = text
            .get(start..end)
            .map_or(0, |before| before.chars().count());
        u32::try_from(before).unwrap_or(u32::MAX).saturating_add(1)
    }
}

/// `message` of a deserialization error, led by where in the value it happened: `` `external[0].cmd`: invalid type … ``. Serde's own message names a field only for an unknown one, and the path names it for every kind. An error at the root of the value has no path to name.
pub fn with_path(path: &serde_path_to_error::Path, message: &str) -> String {
    if path.iter().next().is_none() {
        message.to_owned()
    } else {
        format!("`{path}`: {message}")
    }
}

/// A count with a thousands separator: `4,213`.
pub fn thousands(count: impl TryInto<u64>) -> String {
    let digits = count.try_into().unwrap_or(u64::MAX).to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (position, digit) in digits.chars().enumerate() {
        if position > 0 && (digits.len() - position) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `part` of `whole` as a whole percentage: `29%`. It is rounded down, so that a tree with one lost import never
/// claims to resolve all of them, and it is computed from the counts, because a share kept as a float prints 29 of
/// 100 as `28%`. Nothing out of nothing counts as all of it.
pub fn percent(part: u32, whole: u32) -> String {
    let share = (u64::from(part) * 100)
        .checked_div(u64::from(whole))
        .unwrap_or(100);
    format!("{share}%")
}

/// A count and its noun, for a noun that adds an `s`: `1 file`, `3 files`.
pub fn noun(count: u32, singular: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{} {singular}{plural}", thousands(count))
}

/// Names joined for a sentence: `a`, `a and b`, `a, b and c`.
pub fn join_and(names: &[String]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Names for a heading: the first three as [`join_and`] writes them, then a count of the rest.
pub fn summary(names: &[String]) -> String {
    match names.split_at_checked(3) {
        Some((shown, rest)) if !rest.is_empty() => {
            format!("{} and {} more", shown.join(", "), thousands(rest.len()))
        }
        _ => join_and(names),
    }
}

/// The first seven characters of a commit id, which is how people name one.
pub fn short(head: &str) -> String {
    head.chars().take(7).collect()
}

/// A time in microseconds, written as microseconds, or as milliseconds or seconds with one decimal.
pub fn duration(micros: u64) -> String {
    if micros < 1_000 {
        return format!("{micros} µs");
    }
    // Below this many microseconds a time in milliseconds still rounds to less than a thousand of them.
    if micros < 999_950 {
        format!("{} ms", tenths(micros, 1_000))
    } else {
        format!("{} s", tenths(micros, 1_000_000))
    }
}

/// `micros / unit` rounded to one decimal place, with integers only so that no float's rounding decides it. A
/// `unit` of zero gives `0.0`.
pub fn tenths(micros: u64, unit: u64) -> String {
    let tenths = micros
        .saturating_mul(10)
        .saturating_add(unit / 2)
        .checked_div(unit)
        .unwrap_or(0);
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_map_to_lines() {
        let index = LineIndex::new("ab\ncd\n\nef");
        let lines: Vec<Line> = [0, 2, 3, 6, 7, 99]
            .map(|offset| index.line(offset))
            .to_vec();
        assert_eq!(lines, vec![1, 1, 2, 3, 4, 4]);
    }

    #[test]
    fn a_usize_offset_maps_to_the_same_lines_and_saturates_past_a_u32() {
        let index = LineIndex::new("ab\ncd\n\nef");
        let lines: Vec<Line> = [0, 2, 3, 6, 7, 99]
            .map(|offset| index.line_at(offset))
            .to_vec();
        assert_eq!(lines, vec![1, 1, 2, 3, 4, 4]);
        assert_eq!(index.line_at(usize::MAX), 4);
        assert_eq!(index.line_at(usize::MAX), index.line(u32::MAX));
    }

    #[test]
    fn columns_count_characters_from_one() {
        // `ä` and `ü` are two bytes each: `ab\n` is bytes 0 to 2, `ä` is 3 and 4, and the last `ü` is 9 and 10.
        let text = "ab\näb = ü";
        let index = LineIndex::new(text);
        let columns: Vec<u32> = [0, 1, 2, 3, 4, 5, 9, 10, 11, 99]
            .map(|offset| index.column(text, offset))
            .to_vec();
        assert_eq!(columns, vec![1, 2, 3, 1, 1, 2, 6, 6, 7, 7]);
    }

    #[test]
    fn a_column_of_an_empty_text_is_one() {
        let index = LineIndex::new("");
        assert_eq!(index.column("", 0), 1);
        assert_eq!(index.column("", 7), 1);
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn counts_get_a_thousands_separator() {
        let spelled: Vec<String> = [0_u64, 999, 1_000, 4_213, 1_234_567]
            .into_iter()
            .map(thousands)
            .collect();
        assert_eq!(spelled, ["0", "999", "1,000", "4,213", "1,234,567"]);
    }

    #[test]
    fn a_percentage_is_computed_from_the_counts_and_rounded_down() {
        // 0.29 * 100.0 is 28.999999999999996 as a float, and so are 57, 58 and 69 of 100.
        let spelled: Vec<String> = [(29, 100), (57, 100), (58, 100), (69, 100), (1, 3), (2, 3)]
            .into_iter()
            .map(|(part, whole)| percent(part, whole))
            .collect();
        assert_eq!(spelled, ["29%", "57%", "58%", "69%", "33%", "66%"]);
        assert_eq!(percent(999, 1_000), "99%");
        assert_eq!(percent(0, 5), "0%");
        assert_eq!(percent(5, 5), "100%");
    }

    #[test]
    fn a_percentage_of_nothing_is_all_of_it_and_the_largest_counts_do_not_overflow() {
        assert_eq!(percent(0, 0), "100%");
        assert_eq!(percent(u32::MAX, u32::MAX), "100%");
    }

    #[test]
    fn a_noun_takes_an_s_unless_there_is_one() {
        assert_eq!(noun(1, "file"), "1 file");
        assert_eq!(noun(0, "file"), "0 files");
        assert_eq!(noun(4_213, "file"), "4,213 files");
    }

    #[test]
    fn names_are_joined_for_a_sentence() {
        assert_eq!(join_and(&names(&[])), "");
        assert_eq!(join_and(&names(&["a"])), "a");
        assert_eq!(join_and(&names(&["a", "b"])), "a and b");
        assert_eq!(join_and(&names(&["a", "b", "c"])), "a, b and c");
    }

    #[test]
    fn a_summary_names_three_and_counts_the_rest() {
        assert_eq!(summary(&names(&["a", "b", "c"])), "a, b and c");
        assert_eq!(summary(&names(&["a", "b", "c", "d"])), "a, b, c and 1 more");
    }

    #[test]
    fn a_commit_is_named_by_its_first_seven_characters() {
        assert_eq!(short("a1b2c3d4e5"), "a1b2c3d");
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn durations_pick_a_unit_and_round_once() {
        let spelled: Vec<String> = [
            0, 999, 1_000, 1_049, 1_050, 12_345, 999_949, 999_950, 2_500_000,
        ]
        .into_iter()
        .map(duration)
        .collect();
        assert_eq!(
            spelled,
            [
                "0 µs", "999 µs", "1.0 ms", "1.0 ms", "1.1 ms", "12.3 ms", "999.9 ms", "1.0 s",
                "2.5 s"
            ]
        );
    }

    #[test]
    fn the_largest_duration_does_not_overflow() {
        assert!(duration(u64::MAX).ends_with(" s"));
    }

    #[test]
    fn a_unit_of_zero_gives_zero() {
        assert_eq!(tenths(1_500, 0), "0.0");
    }

    #[test]
    fn an_error_is_led_by_the_path_it_happened_at() {
        #[derive(Debug, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Outer {
            #[expect(dead_code, reason = "read by the deserializer, never by the test")]
            list: Vec<Inner>,
        }
        #[derive(Debug, serde::Deserialize)]
        struct Inner {
            #[expect(dead_code, reason = "read by the deserializer, never by the test")]
            size: u32,
        }
        let message = |value: serde_json::Value| {
            let error = serde_path_to_error::deserialize::<_, Outer>(value).unwrap_err();
            with_path(error.path(), &error.inner().to_string())
        };
        let nested = message(serde_json::json!({ "list": [{ "size": 1 }, { "size": "big" }] }));
        assert!(
            nested.starts_with("`list[1].size`: invalid type"),
            "{nested}"
        );
        // The path of an unknown key is the key, which serde names too.
        let unknown = message(serde_json::json!({ "list": [], "extra": 1 }));
        assert!(
            unknown.starts_with("`extra`: unknown field `extra`"),
            "{unknown}"
        );
        // An error about the whole value has no path to lead with, so it keeps serde's words alone.
        let root = message(serde_json::json!(5));
        assert!(root.starts_with("invalid type: integer `5`"), "{root}");
    }
}
