//! The seeded order of the candidates, and the loop that takes the first ones that qualify.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use anyhow::ensure;

use super::functions::Candidate;
use super::questions::{Kind, Question, prompt};
use super::say;
use crate::digest::sha256_hex;

/// How many other files may refer to a function that a question about its users is drawn for.
const USERS: RangeInclusive<usize> = 3..=30;

/// How many test files may break with a function that a question about tests is drawn for.
pub(super) const BROKEN: RangeInclusive<usize> = 1..=30;

/// What a function that is drawn for adds up to: the paths of its answer, and the test files that count as neither right nor wrong for this function alone.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Answer {
    pub(super) files: Vec<String>,
    pub(super) ignored: Vec<String>,
}

/// Why a candidate was passed over, or the answer when it was not.
pub(super) type Verdict = Result<Answer, &'static str>;

const TOO_FEW_USERS: &str = "too few users";
const TOO_MANY_USERS: &str = "too many users";

/// The SHA-256 of the seed, the repository, the file and the name, separated by NUL bytes, as 64 lower-case hexadecimal digits.
fn hash(seed: &str, repository: &str, candidate: &Candidate) -> String {
    let text = format!(
        "{seed}\0{repository}\0{}\0{}",
        candidate.file, candidate.name
    );
    sha256_hex(text.as_bytes())
}

/// The candidates in the order the draw takes them: by hash, and by file and name where two share one.
pub(super) fn order<'a>(
    seed: &str,
    repository: &str,
    candidates: &'a [Candidate],
) -> Vec<&'a Candidate> {
    let mut keyed: Vec<(String, &Candidate)> = candidates
        .iter()
        .map(|candidate| (hash(seed, repository, candidate), candidate))
        .collect();
    keyed.sort_by(|(a, x), (b, y)| {
        a.cmp(b)
            .then_with(|| x.file.cmp(&y.file))
            .then_with(|| x.name.cmp(&y.name))
    });
    keyed.into_iter().map(|(_, candidate)| candidate).collect()
}

/// What a pass over the candidates found.
pub(super) struct Taken<'a> {
    /// The candidates that qualified, in draw order, with their answers.
    picked: Vec<(&'a Candidate, Answer)>,
    /// How many candidates were looked at.
    tried: usize,
    /// How many were passed over, by reason.
    rejected: BTreeMap<&'static str, usize>,
}

impl Taken<'_> {
    /// One line for the person waiting for the draw.
    fn describe(&self) -> String {
        let rejected = self
            .rejected
            .iter()
            .map(|(why, count)| format!("{count} {why}"))
            .collect::<Vec<_>>()
            .join(", ");
        let tail = if rejected.is_empty() {
            String::new()
        } else {
            format!("; passed over: {rejected}")
        };
        format!("{} tried, {} taken{tail}", self.tried, self.picked.len())
    }
}

/// Walks `order` and keeps the first `count` candidates `judge` gives an answer for, counting why it passed over the others.
fn take<'a>(
    order: &[&'a Candidate],
    count: u32,
    mut judge: impl FnMut(&'a Candidate) -> anyhow::Result<Verdict>,
) -> anyhow::Result<Taken<'a>> {
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    let mut taken = Taken {
        picked: Vec::new(),
        tried: 0,
        rejected: BTreeMap::new(),
    };
    for &candidate in order {
        if taken.picked.len() >= count {
            break;
        }
        taken.tried += 1;
        match judge(candidate)? {
            Ok(answer) => taken.picked.push((candidate, answer)),
            Err(why) => *taken.rejected.entry(why).or_default() += 1,
        }
    }
    Ok(taken)
}

/// Says what the pass found, and fails when it found fewer candidates than the pins ask for questions.
fn found(repository: &str, kind: Kind, wanted: u32, taken: &Taken<'_>) -> anyhow::Result<()> {
    say(&format!(
        "{repository} {}: {}",
        kind.name(),
        taken.describe()
    ))?;
    let picked = taken.picked.len();
    ensure!(
        picked >= usize::try_from(wanted).unwrap_or(usize::MAX),
        "`{repository}` has {picked} {} for questions of the kind `{}`, but the pins ask for {wanted} ({})",
        if picked == 1 {
            "candidate"
        } else {
            "candidates"
        },
        kind.name(),
        taken.describe()
    );
    Ok(())
}

/// The answer of a question about the users of `candidate`, or why it is not drawn: 3 to 30 other files must refer to it.
fn users_answer(candidate: &Candidate) -> Verdict {
    let mut users = candidate.users.clone();
    users.sort();
    users.dedup();
    match users.len() {
        count if count < *USERS.start() => Err(TOO_FEW_USERS),
        count if count > *USERS.end() => Err(TOO_MANY_USERS),
        _ => Ok(Answer {
            files: users,
            ignored: Vec::new(),
        }),
    }
}

/// The questions of `kind`, numbered from 1 in the order they were taken. Each ignores the `ignored` of the repository and its own, once each and in byte order.
fn questions(
    kind: Kind,
    repository: &str,
    scope: &str,
    test: &[String],
    ignored: &BTreeSet<String>,
    picked: &[(&Candidate, Answer)],
) -> Vec<Question> {
    picked
        .iter()
        .zip(1..)
        .map(|((candidate, answer), number)| Question {
            id: format!("{repository}-{}-{number}", kind.name()),
            repository: repository.to_owned(),
            kind,
            scope: scope.to_owned(),
            file: candidate.file.clone(),
            name: candidate.name.clone(),
            line: candidate.line,
            prompt: prompt(kind, &candidate.name, &candidate.file, scope, test),
            answer: answer.files.clone(),
            ignored: ignored
                .iter()
                .chain(&answer.ignored)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .cloned()
                .collect(),
        })
        .collect()
}

/// The questions about the users of functions: the first `count` candidates in `order` that 3 to 30 other files refer to.
pub(super) fn users(
    repository: &str,
    scope: &str,
    count: u32,
    order: &[&Candidate],
) -> anyhow::Result<Vec<Question>> {
    let taken = take(order, count, |candidate| Ok(users_answer(candidate)))?;
    found(repository, Kind::Users, count, &taken)?;
    Ok(questions(
        Kind::Users,
        repository,
        scope,
        &[],
        &BTreeSet::new(),
        &taken.picked,
    ))
}

/// The questions about the tests a function breaks: the first `count` candidates in `order` that `judge` gives an answer for. `ignored` are the test files that fail without any throw, which every question ignores; `judge` adds the ones of one function.
pub(super) fn tests<'a>(
    repository: &str,
    scope: &str,
    count: u32,
    test: &[String],
    ignored: &BTreeSet<String>,
    order: &[&'a Candidate],
    judge: impl FnMut(&'a Candidate) -> anyhow::Result<Verdict>,
) -> anyhow::Result<Vec<Question>> {
    let taken = take(order, count, judge)?;
    found(repository, Kind::Tests, count, &taken)?;
    Ok(questions(
        Kind::Tests,
        repository,
        scope,
        test,
        ignored,
        &taken.picked,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::questions::Questions;

    fn candidate(file: &str, name: &str, users: usize) -> Candidate {
        Candidate {
            file: file.to_owned(),
            name: name.to_owned(),
            line: 1,
            body_offset: Some(10),
            users: (0..users).map(|n| format!("users/u{n}.ts")).collect(),
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "A function with too few or too many users is not drawn"
    )]
    #[test]
    fn only_a_function_with_three_to_thirty_users_becomes_a_question() {
        let candidates = [
            candidate("a.ts", "few", 2),
            candidate("b.ts", "many", 31),
            candidate("c.ts", "right", 5),
        ];
        let order = order("seed", "vite", &candidates);
        let questions = users("vite", "packages/vite", 1, &order).unwrap();
        let names: Vec<&str> = questions.iter().map(|q| q.name.as_str()).collect();
        assert_eq!(names, ["right"]);

        // Asking for more than qualify fails and says how many were found.
        let error = users("vite", "packages/vite", 2, &order)
            .unwrap_err()
            .to_string();
        assert!(error.contains("`vite` has 1 candidate for"), "{error}");
        assert!(
            error.contains("1 too few users") && error.contains("1 too many users"),
            "{error}"
        );
        assert!(error.contains("the pins ask for 2"), "{error}");
    }

    #[test]
    fn the_count_of_candidates_found_is_a_plural_unless_it_is_one() {
        let candidates = [candidate("a.ts", "few", 2)];
        let order = order("seed", "vite", &candidates);
        let error = users("vite", ".", 1, &order).unwrap_err().to_string();
        assert!(error.contains("`vite` has 0 candidates for"), "{error}");
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Candidates are taken in the order of their hash"
    )]
    #[test]
    fn the_candidate_with_the_lower_hash_comes_first() {
        let a = candidate("src/a.ts", "alpha", 3);
        let b = candidate("src/b.ts", "beta", 3);
        // The hashed text is spelled out here, so that its form cannot drift unnoticed.
        assert_eq!(
            hash("seed", "vite", &a),
            sha256_hex(b"seed\0vite\0src/a.ts\0alpha")
        );
        let (low, high) = if hash("seed", "vite", &a) < hash("seed", "vite", &b) {
            (&a, &b)
        } else {
            (&b, &a)
        };
        for candidates in [[a.clone(), b.clone()], [b.clone(), a.clone()]] {
            let order = order("seed", "vite", &candidates);
            let names: Vec<&str> = order.iter().map(|c| c.name.as_str()).collect();
            assert_eq!(names, [low.name.as_str(), high.name.as_str()]);
        }
        // Another seed or another repository is another order of the same candidates.
        let seeds: BTreeSet<String> = (0..8)
            .map(|n| hash(&format!("seed{n}"), "vite", &a))
            .collect();
        assert_eq!(seeds.len(), 8);
        assert_ne!(hash("seed", "vite", &a), hash("seed", "excalidraw", &a));

        // Of many candidates an order that is not the order of the hashes cannot agree by luck.
        let many: Vec<Candidate> = (0..12_usize)
            .map(|n| candidate(&format!("src/f{n}.ts"), &format!("f{n}"), 3))
            .collect();
        let mut by_hash: Vec<&Candidate> = many.iter().collect();
        by_hash.sort_by_cached_key(|c| hash("seed", "vite", c));
        let expected: Vec<&str> = by_hash.iter().map(|c| c.name.as_str()).collect();
        let drawn = order("seed", "vite", &many);
        let names: Vec<&str> = drawn.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, expected);
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The same pins draw the same questions")]
    #[test]
    fn the_same_candidates_and_seed_draw_the_same_file() {
        let candidates: Vec<Candidate> = (0..40_usize)
            .map(|n| candidate(&format!("src/f{n}.ts"), &format!("f{n}"), 1 + n % 8))
            .collect();
        let draw = |candidates: &[Candidate]| {
            let order = order("seed", "vite", candidates);
            Questions {
                seed: "seed".to_owned(),
                questions: users("vite", "packages/vite", 4, &order).unwrap(),
            }
            .render()
            .unwrap()
        };
        let mut reversed = candidates.clone();
        reversed.reverse();
        let first = draw(&candidates);
        assert_eq!(first, draw(&candidates));
        assert_eq!(
            first,
            draw(&reversed),
            "the order of the input must not matter"
        );
    }

    #[test]
    fn questions_are_numbered_from_one_in_draw_order() {
        let candidates: Vec<Candidate> = (0..10_usize)
            .map(|n| candidate(&format!("src/f{n}.ts"), &format!("f{n}"), 4))
            .collect();
        let order = order("seed", "vite", &candidates);
        let questions = users("vite", ".", 3, &order).unwrap();
        let ids: Vec<&str> = questions.iter().map(|q| q.id.as_str()).collect();
        assert_eq!(ids, ["vite-users-1", "vite-users-2", "vite-users-3"]);
        let names: Vec<&str> = questions.iter().map(|q| q.name.as_str()).collect();
        let drawn: Vec<&str> = order.iter().take(3).map(|c| c.name.as_str()).collect();
        assert_eq!(names, drawn);
    }

    #[test]
    fn the_pass_stops_at_the_count_and_counts_what_it_passed_over() {
        let candidates: Vec<Candidate> = (0..6_usize)
            .map(|n| candidate(&format!("f{n}.ts"), &format!("f{n}"), n))
            .collect();
        let order: Vec<&Candidate> = candidates.iter().collect();
        let mut seen = 0;
        let taken = take(&order, 1, |candidate| {
            seen += 1;
            Ok(users_answer(candidate))
        })
        .unwrap();
        // f0, f1 and f2 have 0, 1 and 2 users; f3 has 3 and is the first that qualifies.
        assert_eq!((seen, taken.tried, taken.picked.len()), (4, 4, 1));
        assert_eq!(taken.rejected.get(TOO_FEW_USERS), Some(&3));
        assert!(taken.describe().contains("3 too few users"));
    }

    #[test]
    fn questions_about_tests_carry_the_ignored_files_and_the_command() {
        let candidates = [candidate("src/a.ts", "f", 0)];
        let order: Vec<&Candidate> = candidates.iter().collect();
        let test = ["pnpm", "exec", "vitest", "run"].map(str::to_owned);
        let ignored: BTreeSet<String> = ["cli.spec.ts".to_owned()].into();
        let questions = tests("vite", "packages/vite", 1, &test, &ignored, &order, |_| {
            Ok(Ok(Answer {
                files: vec!["a.spec.ts".to_owned()],
                ignored: vec!["cli.spec.ts".to_owned(), "b.spec.ts".to_owned()],
            }))
        })
        .unwrap();
        let question = questions.first().unwrap();
        assert_eq!(question.id, "vite-tests-1");
        assert_eq!(question.answer, ["a.spec.ts"]);
        // The ignored files of the repository and of the function, once each and in byte order.
        assert_eq!(question.ignored, ["b.spec.ts", "cli.spec.ts"]);
        assert!(question.prompt.contains("`pnpm exec vitest run`"));
    }
}
