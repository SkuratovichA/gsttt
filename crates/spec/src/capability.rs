//! Specs and change deltas: capabilities made of requirements made of scenarios, each with its line.

use std::path::{Path, PathBuf};

use crate::error::{Error, Problem};
use crate::markdown;

/// A `#### Scenario: <title>` heading; a test claims a scenario by its capability and title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    /// The text after `Scenario:`.
    pub title: String,
    /// The line of the heading, counted from 1.
    pub line: usize,
}

/// A `### Requirement: <name>` heading and the scenarios under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The text after `Requirement:`; the `MODIFIED`, `REMOVED` and `RENAMED` sections of a delta refer to a requirement by it.
    pub name: String,
    /// The line of the heading, counted from 1.
    pub line: usize,
    /// The scenarios in the order they are written.
    pub scenarios: Vec<Scenario>,
}

/// A `## Purpose` section: the prose that says what a capability is for, which is empty when the section has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purpose {
    /// The lines of the section that are not blank and not fenced code, trimmed and joined with `\n`; empty when there are none.
    pub text: String,
    /// The line of the first of them, or of the `## Purpose` heading when there are none, counted from 1.
    pub line: usize,
}

/// The requirements of one capability, read from `openspec/specs/<capability>/spec.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// The directory path under `openspec/specs/`, with `/` between nested directories.
    pub capability: String,
    /// The file, relative to the repository root, as it appears in messages.
    pub file: PathBuf,
    /// The `## Purpose` section; `None` when the spec has no such heading, and an empty [`Purpose`] when the section holds no prose.
    pub purpose: Option<Purpose>,
    /// The requirements in the order they are written.
    pub requirements: Vec<Requirement>,
}

/// A requirement that the `REMOVED` section of a delta names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    /// The name of the requirement in the main spec.
    pub name: String,
    /// The line where the delta names it, counted from 1.
    pub line: usize,
}

/// A `FROM` and `TO` pair in the `RENAMED` section of a delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renamed {
    /// The name of the requirement in the main spec.
    pub from: String,
    /// The name it has after the change is archived.
    pub to: String,
    /// The line of the `FROM` entry, counted from 1.
    pub line: usize,
}

/// What one change does to one capability, read from `openspec/changes/<change>/specs/<capability>/spec.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    /// The directory path under the change's `specs/`, with `/` between nested directories.
    pub capability: String,
    /// The file, relative to the repository root, as it appears in messages.
    pub file: PathBuf,
    /// The `## Purpose` section, which OpenSpec reads only when the delta creates the capability; `None` when the delta has no such heading, and an empty [`Purpose`] when the section holds no prose.
    pub purpose: Option<Purpose>,
    /// The requirements of the `ADDED` section.
    pub added: Vec<Requirement>,
    /// The requirements of the `MODIFIED` section, each replacing the main requirement of that name.
    pub modified: Vec<Requirement>,
    /// The requirements of the `REMOVED` section.
    pub removed: Vec<Removed>,
    /// The pairs of the `RENAMED` section.
    pub renamed: Vec<Renamed>,
}

/// Reads a main spec. `file` is only recorded and named in errors; the text comes from `text`.
///
/// # Errors
///
/// Fails with the file and line of the first heading that does not follow the format.
pub fn parse_spec(capability: &str, file: &Path, text: &str) -> Result<Spec, Error> {
    let parser = Parser::read(Kind::Spec, file, text)?;
    Ok(Spec {
        capability: capability.to_owned(),
        file: file.to_path_buf(),
        purpose: parser.purpose,
        requirements: parser.requirements,
    })
}

/// Reads the delta of one change to one capability. `file` is only recorded and named in errors.
///
/// # Errors
///
/// Fails with the file and line of the first heading that does not follow the format, or when the
/// file has none of the four sections.
pub fn parse_delta(capability: &str, file: &Path, text: &str) -> Result<Delta, Error> {
    let parser = Parser::read(Kind::Delta, file, text)?;
    Ok(Delta {
        capability: capability.to_owned(),
        file: file.to_path_buf(),
        purpose: parser.purpose,
        added: parser.added,
        modified: parser.modified,
        removed: parser.removed,
        renamed: parser.renamed,
    })
}

/// Which kind of file is read, which decides the sections that hold requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Spec,
    Delta,
}

impl Kind {
    /// The section a `## ` heading opens in this kind of file.
    fn section(self, title: &str) -> Section {
        if title.eq_ignore_ascii_case("Purpose") {
            return Section::Purpose;
        }
        match self {
            Self::Spec if title.eq_ignore_ascii_case("Requirements") => Section::Requirements,
            Self::Spec => Section::Other,
            Self::Delta => {
                let mut words = title.split_whitespace();
                match (words.next(), words.next(), words.next()) {
                    (Some(name), Some(noun), None) if noun.eq_ignore_ascii_case("Requirements") => {
                        match name.to_ascii_uppercase().as_str() {
                            "ADDED" => Section::Added,
                            "MODIFIED" => Section::Modified,
                            "REMOVED" => Section::Removed,
                            "RENAMED" => Section::Renamed,
                            _ => Section::Other,
                        }
                    }
                    _ => Section::Other,
                }
            }
        }
    }

    /// The headings that may hold a requirement, for the error message.
    fn expected(self) -> &'static str {
        match self {
            Self::Spec => "`## Requirements`",
            Self::Delta => {
                "`## ADDED Requirements`, `## MODIFIED Requirements` or `## REMOVED Requirements`"
            }
        }
    }
}

/// The `## ` section the reader is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// `## Requirements` of a main spec.
    Requirements,
    Added,
    Modified,
    Removed,
    Renamed,
    /// `## Purpose`, whose prose the reader keeps.
    Purpose,
    /// Any other heading, whose content the gate does not read.
    Other,
}

/// A scenario heading whose steps are still being read.
struct OpenScenario {
    title: String,
    line: usize,
    when: bool,
    then: bool,
}

/// Reads one file line by line, the way OpenSpec's own parsers do: fenced code is skipped and a
/// heading closes whatever it ends.
struct Parser<'a> {
    file: &'a Path,
    kind: Kind,
    section: Section,
    saw_section: bool,
    /// The requirement being read and the section it sits in.
    current: Option<(Section, Requirement)>,
    scenario: Option<OpenScenario>,
    /// A `FROM` entry still waiting for its `TO`, with its line.
    from: Option<(String, usize)>,
    purpose: Option<Purpose>,
    requirements: Vec<Requirement>,
    added: Vec<Requirement>,
    modified: Vec<Requirement>,
    removed: Vec<Removed>,
    renamed: Vec<Renamed>,
}

impl<'a> Parser<'a> {
    fn read(kind: Kind, file: &'a Path, text: &str) -> Result<Self, Error> {
        let mut parser = Self {
            file,
            kind,
            section: Section::Other,
            saw_section: false,
            current: None,
            scenario: None,
            from: None,
            purpose: None,
            requirements: Vec::new(),
            added: Vec::new(),
            modified: Vec::new(),
            removed: Vec::new(),
            renamed: Vec::new(),
        };
        let read = markdown::lines(text);
        for line in read.lines.into_iter().filter(|line| !line.fenced) {
            match markdown::heading(line.text) {
                Some((level, title)) => parser.heading(level, title, line.number)?,
                None => parser.body(line.text, line.number)?,
            }
        }
        // Checked before the scenario and the requirement are closed: a fence that hides a step is
        // the cause of the step missing.
        if let Some(line) = read.unclosed {
            return Err(parser.fail(line, Problem::UnclosedFence));
        }
        parser.close_scenario()?;
        parser.close_requirement();
        parser.close_rename()?;
        if kind == Kind::Delta && !parser.saw_section {
            return Err(parser.fail(1, Problem::NoDeltaSection));
        }
        Ok(parser)
    }

    fn fail(&self, line: usize, problem: Problem) -> Error {
        Error::Format {
            file: self.file.to_path_buf(),
            line,
            problem,
        }
    }

    /// Whether requirements with scenarios may sit in the current section.
    fn holds_requirements(&self) -> bool {
        matches!(
            self.section,
            Section::Requirements | Section::Added | Section::Modified
        )
    }

    fn heading(&mut self, level: usize, title: &str, line: usize) -> Result<(), Error> {
        if level <= 4 {
            self.close_scenario()?;
        }
        if level <= 3 {
            self.close_requirement();
        }
        if level <= 2 {
            self.close_rename()?;
        }
        match level {
            2 => {
                self.section = self.kind.section(title);
                self.saw_section |= self.kind == Kind::Delta
                    && !matches!(self.section, Section::Other | Section::Purpose);
                if self.section == Section::Purpose {
                    self.purpose_at(line);
                }
                Ok(())
            }
            3 => self.requirement_heading(title, line),
            4 => self.scenario_heading(title, line),
            _ => Ok(()),
        }
    }

    fn requirement_heading(&mut self, title: &str, line: usize) -> Result<(), Error> {
        let name = markdown::after_label(title, "Requirement:");
        match (name, self.section) {
            (Some(name), Section::Removed) => {
                self.removed.push(Removed {
                    name: name.to_owned(),
                    line,
                });
                Ok(())
            }
            (Some(name), _) if self.holds_requirements() => {
                let requirement = Requirement {
                    name: name.to_owned(),
                    line,
                    scenarios: Vec::new(),
                };
                self.current = Some((self.section, requirement));
                Ok(())
            }
            (Some(name), _) => Err(self.fail(
                line,
                Problem::RequirementOutsideSection {
                    name: name.to_owned(),
                    expected: self.kind.expected(),
                },
            )),
            (None, _) if self.holds_requirements() || self.section == Section::Removed => Err(self
                .fail(
                    line,
                    Problem::RequirementHeading {
                        found: title.to_owned(),
                    },
                )),
            (None, _) => Ok(()),
        }
    }

    fn scenario_heading(&mut self, title: &str, line: usize) -> Result<(), Error> {
        if !self.holds_requirements() {
            return Ok(());
        }
        let Some(scenario) = markdown::after_label(title, "Scenario:") else {
            return Err(self.fail(
                line,
                Problem::ScenarioHeading {
                    found: title.to_owned(),
                },
            ));
        };
        if self.current.is_none() {
            return Err(self.fail(
                line,
                Problem::ScenarioOutsideRequirement {
                    title: scenario.to_owned(),
                },
            ));
        }
        self.scenario = Some(OpenScenario {
            title: scenario.to_owned(),
            line,
            when: false,
            then: false,
        });
        Ok(())
    }

    fn body(&mut self, text: &str, line: usize) -> Result<(), Error> {
        match self.section {
            Section::Requirements | Section::Added | Section::Modified => {
                if let Some(scenario) = &mut self.scenario {
                    match markdown::step_keyword(text) {
                        Some("WHEN") => scenario.when = true,
                        Some("THEN") => scenario.then = true,
                        _ => {}
                    }
                }
                Ok(())
            }
            Section::Removed => {
                if let Some(name) = markdown::requirement_ref(text) {
                    self.removed.push(Removed {
                        name: name.to_owned(),
                        line,
                    });
                }
                Ok(())
            }
            Section::Renamed => self.rename_entry(text, line),
            Section::Purpose => {
                self.purpose_line(text, line);
                Ok(())
            }
            Section::Other => Ok(()),
        }
    }

    /// The purpose being read, started with no prose at `line` when neither the heading nor an earlier line has started it.
    fn purpose_at(&mut self, line: usize) -> &mut Purpose {
        self.purpose.get_or_insert_with(|| Purpose {
            text: String::new(),
            line,
        })
    }

    /// Adds a line of the `## Purpose` section to the prose; a blank line is no prose.
    fn purpose_line(&mut self, text: &str, line: usize) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let purpose = self.purpose_at(line);
        if purpose.text.is_empty() {
            purpose.line = line;
        } else {
            purpose.text.push('\n');
        }
        purpose.text.push_str(text);
    }

    /// Reads a `- FROM: ``### Requirement: Old`` ` or `- TO: …` entry and pairs them in order.
    fn rename_entry(&mut self, text: &str, line: usize) -> Result<(), Error> {
        let Some((side, rest)) = text.split_once(':') else {
            return Ok(());
        };
        let Some(name) = markdown::requirement_ref(rest) else {
            return Ok(());
        };
        let side = side.trim().trim_start_matches(['-', '*', '+']).trim();
        if side.eq_ignore_ascii_case("FROM") {
            if let Some((previous, at)) = self.from.replace((name.to_owned(), line)) {
                return Err(self.fail(
                    at,
                    Problem::UnpairedRename {
                        side: "FROM",
                        name: previous,
                        missing: "TO",
                    },
                ));
            }
        } else if side.eq_ignore_ascii_case("TO") {
            let Some((from, at)) = self.from.take() else {
                return Err(self.fail(
                    line,
                    Problem::UnpairedRename {
                        side: "TO",
                        name: name.to_owned(),
                        missing: "FROM",
                    },
                ));
            };
            self.renamed.push(Renamed {
                from,
                to: name.to_owned(),
                line: at,
            });
        }
        Ok(())
    }

    /// Ends the `RENAMED` section: a `FROM` still open has no `TO`.
    fn close_rename(&mut self) -> Result<(), Error> {
        match self.from.take() {
            Some((name, at)) => Err(self.fail(
                at,
                Problem::UnpairedRename {
                    side: "FROM",
                    name,
                    missing: "TO",
                },
            )),
            None => Ok(()),
        }
    }

    /// Ends the scenario being read: it needs a `WHEN` and a `THEN` step.
    fn close_scenario(&mut self) -> Result<(), Error> {
        let Some(open) = self.scenario.take() else {
            return Ok(());
        };
        for (present, keyword) in [(open.when, "WHEN"), (open.then, "THEN")] {
            if !present {
                return Err(self.fail(
                    open.line,
                    Problem::ScenarioStep {
                        title: open.title,
                        keyword,
                    },
                ));
            }
        }
        if let Some((_, requirement)) = &mut self.current {
            requirement.scenarios.push(Scenario {
                title: open.title,
                line: open.line,
            });
        }
        Ok(())
    }

    /// Ends the requirement being read and files it under the section it sits in.
    fn close_requirement(&mut self) {
        let Some((section, requirement)) = self.current.take() else {
            return;
        };
        match section {
            Section::Requirements => self.requirements.push(requirement),
            Section::Added => self.added.push(requirement),
            Section::Modified => self.modified.push(requirement),
            Section::Removed | Section::Renamed | Section::Purpose | Section::Other => {}
        }
    }
}
