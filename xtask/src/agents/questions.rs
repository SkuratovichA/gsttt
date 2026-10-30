//! The questions: `bench/agents/questions.json` is the contract between the draw and the runs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context as _, bail, ensure};
use serde::{Deserialize, Serialize};

/// What a question asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    /// Which other files refer to a function.
    Users,
    /// Which test files fail when a function throws.
    Tests,
    /// Which files send a GraphQL operation whose resolver calls a method.
    Clients,
}

impl Kind {
    /// The word the file and the ids use for the kind.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::Tests => "tests",
            Self::Clients => "clients",
        }
    }
}

/// One question and its answer.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Question {
    /// `<repository>-<kind>-<n>`, with `n` counting from 1 in draw order within the repository and the kind.
    pub(crate) id: String,
    pub(crate) repository: String,
    pub(crate) kind: Kind,
    /// The directory the question is about, relative to the checkout; `.` for all of it.
    pub(crate) scope: String,
    /// The file that exports the function, or defines the class of the method, relative to the checkout.
    pub(crate) file: String,
    /// The name of the function, or `Class.method` for a method.
    pub(crate) name: String,
    /// The line of the name in the file, from 1.
    pub(crate) line: u32,
    /// What the agent is asked, word for word.
    pub(crate) prompt: String,
    /// The paths the agent should list, relative to the checkout, sorted.
    pub(crate) answer: Vec<String>,
    /// The test files that are neither right nor wrong in an answer, sorted: the ones that failed before the throw, and the ones that failed in only one of the two runs with it. Empty for the kinds `users` and `clients`.
    pub(crate) ignored: Vec<String>,
}

/// The questions of a draw, and the seed they were drawn with.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Questions {
    pub(crate) seed: String,
    /// Repository by repository in pin order, the kinds in the order `users`, `tests`, `clients`, each in draw order.
    pub(crate) questions: Vec<Question>,
}

impl Questions {
    /// Reads and checks the questions in `path`.
    pub(crate) fn load(path: &Path) -> anyhow::Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let questions: Self = serde_json::from_str(&text)
            .with_context(|| format!("{} does not hold valid questions", path.display()))?;
        questions
            .check()
            .with_context(|| format!("{} does not hold valid questions", path.display()))?;
        Ok(questions)
    }

    /// The text of the file: pretty JSON with a final newline, the same bytes for the same questions.
    pub(crate) fn render(&self) -> anyhow::Result<String> {
        self.check()?;
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    /// Refuses questions that are not in the order, numbering and form the runs rely on.
    fn check(&self) -> anyhow::Result<()> {
        ensure!(!self.seed.trim().is_empty(), "the questions have no seed");
        let mut numbers: BTreeMap<(&str, Kind), u32> = BTreeMap::new();
        let mut finished: BTreeSet<&str> = BTreeSet::new();
        let mut previous: Option<(&str, Kind)> = None;
        for question in &self.questions {
            let id = &question.id;
            let repository = question.repository.as_str();
            if let Some((last, kind)) = previous {
                if last == repository {
                    ensure!(
                        kind <= question.kind,
                        "{id}: a question of the kind `{}` follows one of the kind `{}` in `{repository}`",
                        question.kind.name(),
                        kind.name()
                    );
                } else {
                    finished.insert(last);
                    ensure!(
                        !finished.contains(repository),
                        "{id}: the questions of `{repository}` are not together"
                    );
                }
            }
            previous = Some((repository, question.kind));

            let number = numbers.entry((repository, question.kind)).or_default();
            *number += 1;
            let expected = format!("{repository}-{}-{number}", question.kind.name());
            ensure!(*id == expected, "the id `{id}` should be `{expected}`");
            ensure!(
                !question.prompt.trim().is_empty() && !question.answer.is_empty(),
                "{id}: it has no prompt or no answer"
            );
            ensure!(
                question.kind == Kind::Tests || question.ignored.is_empty(),
                "{id}: only a question about tests ignores files"
            );
            if let Some(path) = question
                .answer
                .iter()
                .find(|path| question.ignored.contains(path))
            {
                bail!("{id}: `{path}` is in the answer and in the ignored files");
            }
            for (what, paths) in [("answer", &question.answer), ("ignored", &question.ignored)] {
                ensure!(
                    paths.is_sorted_by(|a, b| a < b),
                    "{id}: the {what} is not sorted, or lists a path twice"
                );
            }
        }
        Ok(())
    }
}

/// How every prompt that asks for files ends.
const CLOSING: &str = "Do not change any file. End your reply with a fenced code block marked `files` that lists one path per line, relative to the repository root, and nothing else.";

/// Where a question looks for files: all of the repository, or the directory of its scope.
fn place(scope: &str) -> String {
    if scope == "." {
        "of the repository".to_owned()
    } else {
        format!("under `{scope}`")
    }
}

/// What the agent is asked about the function or method `name` of `file`: the files of `scope` that refer to the function, the test files, of the unit tests that `test` runs, that fail when the function fails every time it is called: by throwing an error or, for an async function, by returning a promise that rejects with one, or the files of `scope` that hold a GraphQL operation whose resolver calls the method.
pub(crate) fn prompt(kind: Kind, name: &str, file: &str, scope: &str, test: &[String]) -> String {
    match kind {
        Kind::Users => {
            let place = place(scope);
            format!(
                "In this repository, the function `{name}` is exported from `{file}`. List every other TypeScript file {place} that refers to `{name}` by name: a file that imports it, re-exports it by name, or uses it. {CLOSING}"
            )
        }
        Kind::Tests => format!(
            "In this repository, suppose the function `{name}` in `{file}` failed every time it is called, by throwing an error or, if it is an async function, by returning a promise that rejects with one. Which test files of the unit tests that `{}` runs from the repository root would fail? Do not change any file and do not run any tests. End your reply with a fenced code block marked `files` that lists one path per line, relative to the repository root, and nothing else.",
            test.join(" ")
        ),
        Kind::Clients => {
            let place = place(scope);
            format!(
                "In this repository, the method `{name}` is defined in `{file}`. Which files {place} hold a GraphQL operation that selects a field whose resolver calls `{name}` directly? A resolver is a method of a class decorated with `@Resolver` that is itself decorated with `@Query`, `@Mutation` or `@Subscription`, and it serves the field its decorator names with `name` or with a string argument, else the field of its own name. A file holds an operation when it is a `.graphql` or `.gql` file, or has a `gql` or `graphql` template, with the operation in it; the operation selects the field at its top level, by its name, under an alias or through a fragment. {CLOSING}"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(repository: &str, kind: Kind, number: u32) -> Question {
        Question {
            id: format!("{repository}-{}-{number}", kind.name()),
            repository: repository.to_owned(),
            kind,
            scope: ".".to_owned(),
            file: "src/a.ts".to_owned(),
            name: "f".to_owned(),
            line: 3,
            prompt: "p".to_owned(),
            answer: vec!["a.spec.ts".to_owned(), "b.spec.ts".to_owned()],
            ignored: Vec::new(),
        }
    }

    fn sample() -> Questions {
        Questions {
            seed: "seed".to_owned(),
            questions: vec![
                question("vite", Kind::Users, 1),
                question("vite", Kind::Users, 2),
                question("vite", Kind::Tests, 1),
                question("excalidraw", Kind::Users, 1),
            ],
        }
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The prompt asks for TypeScript files")]
    #[test]
    fn the_prompt_about_users_is_word_for_word() {
        let command = prompt(
            Kind::Users,
            "resolveId",
            "packages/vite/src/a.ts",
            "packages/vite",
            &[],
        );
        assert_eq!(
            command,
            "In this repository, the function `resolveId` is exported from `packages/vite/src/a.ts`. List every other TypeScript file under `packages/vite` that refers to `resolveId` by name: a file that imports it, re-exports it by name, or uses it. Do not change any file. End your reply with a fenced code block marked `files` that lists one path per line, relative to the repository root, and nothing else."
        );
        let whole = prompt(Kind::Users, "f", "a.ts", ".", &[]);
        assert!(
            whole.contains(
                "List every other TypeScript file of the repository that refers to `f` by name"
            ),
            "{whole}"
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The prompt asks which files send an operation")]
    #[test]
    fn the_prompt_about_clients_is_word_for_word() {
        let command = prompt(
            Kind::Clients,
            "AdminService.addUserToTeam",
            "packages/hoppscotch-backend/src/admin/admin.service.ts",
            "packages",
            &[],
        );
        assert_eq!(
            command,
            "In this repository, the method `AdminService.addUserToTeam` is defined in `packages/hoppscotch-backend/src/admin/admin.service.ts`. Which files under `packages` hold a GraphQL operation that selects a field whose resolver calls `AdminService.addUserToTeam` directly? A resolver is a method of a class decorated with `@Resolver` that is itself decorated with `@Query`, `@Mutation` or `@Subscription`, and it serves the field its decorator names with `name` or with a string argument, else the field of its own name. A file holds an operation when it is a `.graphql` or `.gql` file, or has a `gql` or `graphql` template, with the operation in it; the operation selects the field at its top level, by its name, under an alias or through a fragment. Do not change any file. End your reply with a fenced code block marked `files` that lists one path per line, relative to the repository root, and nothing else."
        );
        let whole = prompt(Kind::Clients, "A.b", "a.ts", ".", &[]);
        assert!(
            whole.contains("Which files of the repository hold"),
            "{whole}"
        );
        // Every prompt that asks for files ends the same way.
        let users = prompt(Kind::Users, "f", "a.ts", ".", &[]);
        assert!(users.ends_with(CLOSING) && whole.ends_with(CLOSING));
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "The prompt says how the function fails")]
    #[test]
    fn the_prompt_about_tests_is_word_for_word() {
        let test = ["pnpm", "exec", "vitest", "run"].map(str::to_owned);
        assert_eq!(
            prompt(Kind::Tests, "f", "src/a.ts", "packages/vite", &test),
            "In this repository, suppose the function `f` in `src/a.ts` failed every time it is called, by throwing an error or, if it is an async function, by returning a promise that rejects with one. Which test files of the unit tests that `pnpm exec vitest run` runs from the repository root would fail? Do not change any file and do not run any tests. End your reply with a fenced code block marked `files` that lists one path per line, relative to the repository root, and nothing else."
        );
    }

    #[test]
    fn questions_survive_the_file() {
        let questions = sample();
        let text = questions.render().unwrap();
        assert!(text.ends_with("}\n"));
        assert!(text.contains("\"repository\": \"vite\""));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("questions.json");
        fs::write(&path, &text).unwrap();
        assert_eq!(Questions::load(&path).unwrap(), questions);
    }

    #[test]
    fn the_fields_are_camel_case_in_the_order_the_runs_read_them() {
        let text = sample().render().unwrap();
        let fields = [
            "id",
            "repository",
            "kind",
            "scope",
            "file",
            "name",
            "line",
            "prompt",
            "answer",
            "ignored",
        ];
        let positions: Vec<usize> = fields
            .iter()
            .map(|field| text.find(&format!("\"{field}\":")).unwrap())
            .collect();
        assert!(positions.is_sorted(), "{positions:?}");
    }

    #[test]
    fn questions_out_of_order_or_number_are_refused() {
        let mut wrong_number = sample();
        wrong_number.questions.swap(0, 1);
        let mut users_after_tests = sample();
        users_after_tests.questions.swap(1, 2);
        let mut split = sample();
        split.questions.push(question("vite", Kind::Tests, 2));
        let mut unsorted = sample();
        if let Some(first) = unsorted.questions.first_mut() {
            first.answer.reverse();
        }
        let mut ignored_users = sample();
        if let Some(first) = ignored_users.questions.first_mut() {
            first.ignored.push("x.spec.ts".to_owned());
        }
        let mut ignored_clients = clients_after_tests();
        if let Some(last) = ignored_clients.questions.last_mut() {
            last.ignored.push("x.spec.ts".to_owned());
        }
        for (what, questions) in [
            ("numbers", wrong_number),
            ("kinds", users_after_tests),
            ("ignored clients", ignored_clients),
            ("repositories", split),
            ("sorting", unsorted),
            ("ignored users", ignored_users),
        ] {
            assert!(questions.render().is_err(), "{what}");
        }
    }

    #[test]
    fn the_committed_questions_are_valid_and_the_clients_come_last() {
        let root = crate::root::repository_root().unwrap();
        let questions = Questions::load(&root.join(crate::agents::QUESTIONS)).unwrap();
        let kinds: Vec<(&str, Kind)> = questions
            .questions
            .iter()
            .map(|question| (question.repository.as_str(), question.kind))
            .collect();
        assert!(kinds.contains(&("hoppscotch", Kind::Clients)));
        assert!(kinds.contains(&("vite", Kind::Tests)));
        assert_eq!(kinds.last(), Some(&("hoppscotch", Kind::Clients)));
        for question in questions
            .questions
            .iter()
            .filter(|question| question.kind == Kind::Clients)
        {
            assert!(!question.answer.is_empty() && question.answer.len() <= 30);
            assert!(question.prompt.contains(&format!("`{}`", question.name)));
        }
    }

    /// The sample with the questions about clients of `vite` after the ones about tests.
    fn clients_after_tests() -> Questions {
        let mut questions = sample();
        questions
            .questions
            .insert(3, question("vite", Kind::Clients, 1));
        questions
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Questions about clients come after the others of their repository"
    )]
    #[test]
    fn questions_about_clients_follow_those_about_users_and_tests() {
        let questions = clients_after_tests();
        let text = questions.render().unwrap();
        assert!(text.contains("\"kind\": \"clients\""), "{text}");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("questions.json");
        fs::write(&path, &text).unwrap();
        assert_eq!(Questions::load(&path).unwrap(), questions);
        assert!(Kind::Users < Kind::Tests && Kind::Tests < Kind::Clients);

        // Between the questions about users and tests of a repository, or before them, they are refused.
        let mut between = sample();
        between
            .questions
            .insert(2, question("vite", Kind::Clients, 1));
        let error = between.render().unwrap_err().to_string();
        assert!(
            error.contains("`tests` follows one of the kind `clients`"),
            "{error}"
        );
    }

    #[gsttt_scenario::scenario("agent-benchmarks", "A file is never right and ignored at once")]
    #[test]
    fn a_file_in_the_answer_and_in_the_ignored_files_is_refused() {
        let mut questions = sample();
        let tests = questions
            .questions
            .iter_mut()
            .find(|question| question.kind == Kind::Tests)
            .unwrap();
        tests.ignored = vec!["c.spec.ts".to_owned()];
        assert!(
            questions.render().is_ok(),
            "files of the answer and files ignored that differ are valid"
        );

        let tests = questions
            .questions
            .iter_mut()
            .find(|question| question.kind == Kind::Tests)
            .unwrap();
        tests.ignored = vec!["b.spec.ts".to_owned(), "c.spec.ts".to_owned()];
        let error = questions.render().unwrap_err().to_string();
        assert!(error.contains("vite-tests-1"), "{error}");
        assert!(error.contains("`b.spec.ts`"), "{error}");
        assert!(error.contains("answer"), "{error}");
        assert!(error.contains("ignored"), "{error}");
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let text = sample()
            .render()
            .unwrap()
            .replace("\"line\": 3,", "\"line\": 3, \"extra\": 1,");
        assert!(serde_json::from_str::<Questions>(&text).is_err());
    }
}
