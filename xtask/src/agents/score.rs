//! Reading the answer of a run and scoring it.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// What the answer holds in place of a path outside the checkout: such a path is a path of this machine, and the results are published.
fn stand_in(number: usize) -> String {
    format!("<outside the checkout #{number}>")
}

/// The lines of the last block of `message` marked `files`, or `None` when it has none. A block that is never closed is not one.
pub(super) fn last_files_block(message: &str) -> Option<Vec<String>> {
    let mut last = None;
    // The length of the fence that opened the block being read, whether the block is marked `files`, and its lines so far.
    let mut open: Option<(usize, bool, Vec<String>)> = None;
    for line in message.lines() {
        let line = line.trim();
        let ticks = line.chars().take_while(|&c| c == '`').count();
        open = match open.take() {
            None if ticks >= 3 => Some((
                ticks,
                line.trim_start_matches('`').trim() == "files",
                Vec::new(),
            )),
            None => None,
            Some((length, files, lines)) if ticks >= length && ticks == line.chars().count() => {
                if files {
                    last = Some(lines);
                }
                None
            }
            Some((length, files, mut lines)) => {
                lines.push(line.to_owned());
                Some((length, files, lines))
            }
        };
    }
    last
}

/// A line of a block as the path it names: the bullet and the quotes some answers put around it are dropped.
fn clean(line: &str) -> &str {
    let line = line.trim();
    let line = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .unwrap_or(line);
    line.trim().trim_matches(['`', '"', '\''])
}

/// The path relative to the checkout, or `None` when it is outside: an absolute path counts as relative when it is under one of the `roots` of the checkout (the temporary directory of macOS is behind a link, so a checkout has two names), and `..` may not climb above the root.
fn relative(path: &str, roots: &[PathBuf]) -> Option<String> {
    let path = Path::new(path);
    let inside = if path.is_absolute() {
        roots.iter().find_map(|root| path.strip_prefix(root).ok())?
    } else {
        path
    };
    let mut parts = Vec::new();
    for component in inside.components() {
        match component {
            Component::Normal(name) => parts.push(name.to_str()?),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The paths of `lines`, relative to the checkout, each kept once and in byte order. A path outside the checkout is a wrong answer that is not kept: it is replaced by a stand-in, one for each path.
pub(super) fn normalize(lines: &[String], roots: &[PathBuf]) -> Vec<String> {
    let mut answer = BTreeSet::new();
    let mut outside = BTreeSet::new();
    for line in lines
        .iter()
        .map(|line| clean(line))
        .filter(|line| !line.is_empty())
    {
        match relative(line, roots) {
            Some(path) => {
                answer.insert(path);
            }
            None => {
                outside.insert(line);
            }
        }
    }
    answer.extend((1..=outside.len()).map(stand_in));
    answer.into_iter().collect()
}

/// How right an answer is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Score {
    pub(super) precision: f64,
    pub(super) recall: f64,
    pub(super) f1: f64,
}

impl Score {
    /// What a run without an answer scores.
    pub(super) const ZERO: Self = Self {
        precision: 0.0,
        recall: 0.0,
        f1: 0.0,
    };
}

/// Scores `answer` against the `expected` files. The `ignored` files, which fail without the throw, are neither right nor wrong, so they are taken out of the answer. A path outside the scope is simply not in `expected`, which makes it wrong.
pub(super) fn score(answer: &[String], expected: &[String], ignored: &[String]) -> Score {
    let answer: BTreeSet<&String> = answer
        .iter()
        .filter(|path| !ignored.contains(path))
        .collect();
    let expected: BTreeSet<&String> = expected.iter().collect();
    let right = answer.intersection(&expected).count();
    Score {
        precision: ratio(right, answer.len()),
        recall: ratio(right, expected.len()),
        f1: ratio(2 * right, answer.len() + expected.len()),
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    float(numerator) / float(denominator)
}

/// A count as a float. The counts here, files of an answer and tokens and milliseconds of a run, are far below 2^53, where a float stops being exact; a larger one saturates.
#[expect(clippy::cast_precision_loss, reason = "the counts are far below 2^53")]
pub(super) fn float<T: TryInto<u64>>(number: T) -> f64 {
    number.try_into().unwrap_or(u64::MAX) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The last files block is the answer")]
    #[test]
    fn the_last_files_block_is_the_answer() {
        let message = "Here:\n```files\nfirst.ts\n```\nand then\n```ts\nconst x = 1;\n```\nfinally\n```files\nsecond.ts\nthird.ts\n```\n";
        assert_eq!(
            last_files_block(message),
            Some(strings(&["second.ts", "third.ts"]))
        );
        assert_eq!(last_files_block("```ts\nconst x = 1;\n```"), None);
        assert_eq!(
            last_files_block("```files\nunclosed.ts"),
            None,
            "a block that is never closed is not an answer"
        );
        assert_eq!(
            last_files_block("```files\n```"),
            Some(Vec::new()),
            "an empty block is an answer with no file"
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Paths in the checkout count as relative")]
    #[test]
    fn paths_in_the_checkout_count_as_relative() {
        let roots = [
            PathBuf::from("/private/var/work/vite"),
            PathBuf::from("/var/work/vite"),
        ];
        let lines = strings(&[
            "/private/var/work/vite/src/a.ts",
            "./src/a.ts",
            "src/b.ts",
            "/var/work/vite/src/b.ts",
            "- `src/b.ts`",
        ]);
        assert_eq!(normalize(&lines, &roots), ["src/a.ts", "src/b.ts"]);
    }

    #[test]
    fn a_path_that_climbs_out_of_the_checkout_is_outside() {
        let roots = [PathBuf::from("/work/vite")];
        let lines = strings(&["../other/a.ts", "src/../b.ts", "/etc/passwd"]);
        assert_eq!(
            normalize(&lines, &roots),
            [
                "<outside the checkout #1>",
                "<outside the checkout #2>",
                "b.ts"
            ]
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "Precision, recall and F1")]
    #[test]
    fn precision_recall_and_f1() {
        let scored = score(
            &strings(&["a.ts", "b.ts", "x.ts"]),
            &strings(&["a.ts", "b.ts", "c.ts", "d.ts"]),
            &[],
        );
        assert!((scored.precision - 2.0 / 3.0).abs() < 1e-12, "{scored:?}");
        assert!((scored.recall - 0.5).abs() < 1e-12, "{scored:?}");
        assert!((scored.f1 - 4.0 / 7.0).abs() < 1e-12, "{scored:?}");
    }

    #[test]
    fn a_file_that_fails_without_the_throw_is_neither_right_nor_wrong() {
        let scored = score(
            &strings(&["a.spec.ts", "flaky.spec.ts"]),
            &strings(&["a.spec.ts"]),
            &strings(&["flaky.spec.ts"]),
        );
        assert_eq!(
            scored,
            Score {
                precision: 1.0,
                recall: 1.0,
                f1: 1.0
            }
        );
    }

    #[test]
    fn an_empty_answer_scores_zero() {
        assert_eq!(score(&[], &strings(&["a.ts"]), &[]), Score::ZERO);
    }
}
