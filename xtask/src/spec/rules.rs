//! The rules of the gate.
//!
//! A scenario needs a proof when the specs will have it once the open changes are archived: the
//! main scenarios, except those of requirements that an open change removes or modifies, and the
//! scenarios of the `ADDED` and `MODIFIED` requirements of the open changes. Reading the specs that
//! way keeps a modified scenario from counting twice, and makes the tests follow a change as it
//! lands: once a change removes or retitles a scenario, no test may keep claiming the old one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gsttt_spec::{Change, Delta, OpenSpec, Requirement, Scenario};

use super::proofs::{Claim, Marks, Site};

/// Where a scenario is written.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    /// `openspec/specs/`.
    Main,
    /// The `ADDED` or `MODIFIED` requirements of the open change with this name.
    Change(String),
}

/// What the gate makes of a scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    /// Exactly one test claims it.
    Proven,
    /// An open change with tasks left adds it and no test claims it yet.
    Pending,
    /// The gate fails because of it.
    Failing,
}

/// A scenario the specs have, or will have once the open changes are archived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Entry {
    pub(super) capability: String,
    pub(super) title: String,
    pub(super) source: Source,
    /// The line of the scenario's heading.
    pub(super) site: Site,
    pub(super) status: Status,
    /// The tests that claim it.
    pub(super) proofs: Vec<Claim>,
}

impl Entry {
    fn new(capability: &str, scenario: &Scenario, source: Source, file: &Path) -> Self {
        Self {
            capability: capability.to_owned(),
            title: scenario.title.clone(),
            source,
            site: Site {
                file: file.to_path_buf(),
                line: scenario.line,
            },
            status: Status::Failing,
            proofs: Vec::new(),
        }
    }

    /// The open change that adds the scenario, if one does.
    pub(super) fn change(&self) -> Option<&str> {
        match &self.source {
            Source::Main => None,
            Source::Change(name) => Some(name.as_str()),
        }
    }
}

/// One reason for the gate to fail.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Failure {
    /// The file and line to look at first.
    pub(super) site: Site,
    pub(super) message: String,
    /// The open changes the failure holds back from archiving.
    pub(super) changes: Vec<String>,
}

/// An open change, as far as archiving cares.
#[derive(Debug)]
struct Open {
    name: String,
    finished: bool,
    /// Whether the main specs have every requirement that the change modifies, removes or renames:
    /// `openspec archive` stops at one that is missing.
    applies: bool,
}

/// Everything the gate found out.
#[derive(Debug)]
pub(super) struct Judgement {
    /// Sorted by capability, then main scenarios before changes, then by place.
    pub(super) entries: Vec<Entry>,
    /// Sorted by place.
    pub(super) failures: Vec<Failure>,
    changes: Vec<Open>,
}

impl Judgement {
    /// The gate passes exactly when nothing failed.
    pub(super) fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// The open changes that can be archived: every task ticked, every scenario proven, every
    /// requirement they modify, remove or rename in the main specs, and no failure that mentions
    /// them. In the order of their names.
    pub(super) fn ready(&self) -> Vec<&str> {
        self.changes
            .iter()
            .filter(|change| change.finished && change.applies)
            .filter(|change| {
                self.entries
                    .iter()
                    .filter(|entry| entry.change() == Some(change.name.as_str()))
                    .all(|entry| entry.status == Status::Proven)
            })
            .filter(|change| {
                !self
                    .failures
                    .iter()
                    .any(|failure| failure.changes.contains(&change.name))
            })
            .map(|change| change.name.as_str())
            .collect()
    }
}

/// How an open change takes a main requirement away.
#[derive(Debug, Clone, Copy)]
enum Replacement {
    Removed,
    Modified,
}

/// The main requirements that open changes remove or modify, by capability and name, with the
/// first change (in name order) that does.
type Replaced = BTreeMap<(String, String), (String, Replacement)>;

/// A main scenario that an open change takes away.
#[derive(Debug)]
struct Dropped {
    change: String,
    requirement: String,
    how: Replacement,
}

/// Judges the scenarios of the specs against the claims found in the sources.
pub(super) fn judge(openspec: &OpenSpec, marks: &Marks) -> Judgement {
    let (mut entries, dropped) = effective(openspec);
    let unfinished: BTreeMap<&str, bool> = openspec
        .changes
        .iter()
        .map(|change| (change.name.as_str(), !change.is_finished()))
        .collect();

    let mut homes: BTreeMap<(String, String), Vec<(Site, Source)>> = BTreeMap::new();
    for entry in &entries {
        homes
            .entry((entry.capability.clone(), entry.title.clone()))
            .or_default()
            .push((entry.site.clone(), entry.source.clone()));
    }
    let mut claims: BTreeMap<(&str, &str), Vec<&Claim>> = BTreeMap::new();
    for claim in &marks.claims {
        claims
            .entry((claim.capability.as_str(), claim.scenario.as_str()))
            .or_default()
            .push(claim);
    }

    let mut failures = Vec::new();
    for ((capability, title), places) in &homes {
        if places.len() > 1 {
            failures.push(duplicate(capability, title, places));
        }
    }
    for entry in &mut entries {
        let duplicated = homes
            .get(&(entry.capability.clone(), entry.title.clone()))
            .is_some_and(|places| places.len() > 1);
        entry.proofs = claims
            .get(&(entry.capability.as_str(), entry.title.as_str()))
            .map_or_else(Vec::new, |found| {
                found.iter().map(|claim| (*claim).clone()).collect()
            });
        entry.status = if duplicated {
            Status::Failing
        } else {
            status(entry, &unfinished, &mut failures)
        };
    }
    for claim in &marks.claims {
        if !homes.contains_key(&(claim.capability.clone(), claim.scenario.clone())) {
            failures.push(orphan(claim, &dropped));
        }
    }
    for misuse in &marks.misuses {
        failures.push(Failure {
            site: misuse.site.clone(),
            message: misuse.message.clone(),
            changes: Vec::new(),
        });
    }

    entries.sort_by(|a, b| {
        (&a.capability, &a.source, &a.site).cmp(&(&b.capability, &b.source, &b.site))
    });
    failures.sort();
    let changes = openspec
        .changes
        .iter()
        .map(|change| Open {
            name: change.name.clone(),
            finished: change.is_finished(),
            applies: change.deltas.iter().all(|delta| applies(openspec, delta)),
        })
        .collect();
    Judgement {
        entries,
        failures,
        changes,
    }
}

/// The scenarios that need a proof, and the main scenarios that open changes take away.
fn effective(openspec: &OpenSpec) -> (Vec<Entry>, BTreeMap<(String, String), Dropped>) {
    let replaced = replaced(&openspec.changes);
    let mut entries = Vec::new();
    let mut dropped = BTreeMap::new();
    for spec in &openspec.specs {
        for requirement in &spec.requirements {
            let taken = replaced.get(&(spec.capability.clone(), requirement.name.clone()));
            for scenario in &requirement.scenarios {
                if let Some((change, how)) = taken {
                    dropped.insert(
                        (spec.capability.clone(), scenario.title.clone()),
                        Dropped {
                            change: change.clone(),
                            requirement: requirement.name.clone(),
                            how: *how,
                        },
                    );
                } else {
                    entries.push(Entry::new(
                        &spec.capability,
                        scenario,
                        Source::Main,
                        &spec.file,
                    ));
                }
            }
        }
    }
    for change in &openspec.changes {
        for delta in &change.deltas {
            for requirement in delta.added.iter().chain(&delta.modified) {
                for scenario in &requirement.scenarios {
                    entries.push(Entry::new(
                        &delta.capability,
                        scenario,
                        Source::Change(change.name.clone()),
                        &delta.file,
                    ));
                }
            }
        }
    }
    (entries, dropped)
}

fn replaced(changes: &[Change]) -> Replaced {
    let mut replaced = Replaced::new();
    for change in changes {
        for delta in &change.deltas {
            let removals = delta
                .removed
                .iter()
                .map(|removed| (&removed.name, Replacement::Removed));
            let modifications = delta
                .modified
                .iter()
                .map(|requirement| (main_name(delta, requirement), Replacement::Modified));
            for (name, how) in removals.chain(modifications) {
                replaced
                    .entry((delta.capability.clone(), name.clone()))
                    .or_insert_with(|| (change.name.clone(), how));
            }
        }
    }
    replaced
}

/// The name under which the main spec has a requirement that `delta` modifies. A requirement that
/// is renamed and modified appears under its new name in the delta and under its old name in the
/// main spec.
fn main_name<'a>(delta: &'a Delta, modified: &'a Requirement) -> &'a String {
    delta
        .renamed
        .iter()
        .find(|renamed| renamed.to == modified.name)
        .map_or(&modified.name, |renamed| &renamed.from)
}

/// Whether the main spec of the capability of `delta` has every requirement that `delta` modifies,
/// removes or renames. A capability with no main spec has none.
fn applies(openspec: &OpenSpec, delta: &Delta) -> bool {
    let main: BTreeSet<&String> = openspec
        .specs
        .iter()
        .filter(|spec| spec.capability == delta.capability)
        .flat_map(|spec| &spec.requirements)
        .map(|requirement| &requirement.name)
        .collect();
    let removed = delta.removed.iter().map(|removed| &removed.name);
    let renamed = delta.renamed.iter().map(|renamed| &renamed.from);
    let modified = delta
        .modified
        .iter()
        .map(|requirement| main_name(delta, requirement));
    removed
        .chain(renamed)
        .chain(modified)
        .all(|name| main.contains(name))
}

/// The status of a scenario that is written once, with the failure when it fails.
fn status(entry: &Entry, unfinished: &BTreeMap<&str, bool>, failures: &mut Vec<Failure>) -> Status {
    let capability = &entry.capability;
    let title = &entry.title;
    let changes: Vec<String> = entry.change().map(str::to_owned).into_iter().collect();
    let mut fail = |message: String| {
        failures.push(Failure {
            site: entry.site.clone(),
            message,
            changes: changes.clone(),
        });
        Status::Failing
    };
    match (entry.proofs.len(), entry.change()) {
        (1, _) => Status::Proven,
        (0, Some(name)) if unfinished.get(name).copied().unwrap_or(false) => Status::Pending,
        (0, Some(name)) => fail(format!(
            "scenario `{title}` of capability `{capability}` has no proof, and every task of change `{name}` is ticked"
        )),
        (0, None) => fail(format!(
            "scenario `{title}` of capability `{capability}` has no proof; mark a test with #[scenario(\"{capability}\", \"{title}\")]"
        )),
        (count, _) => {
            let proofs = join(entry.proofs.iter().map(ToString::to_string));
            fail(format!(
                "scenario `{title}` of capability `{capability}` has {count} proofs but needs exactly one: {proofs}"
            ))
        }
    }
}

fn duplicate(capability: &str, title: &str, places: &[(Site, Source)]) -> Failure {
    let mut places = places.to_vec();
    places.sort();
    let sites = join(places.iter().map(|(site, _)| site.to_string()));
    Failure {
        site: places
            .first()
            .map(|(site, _)| site.clone())
            .unwrap_or_default(),
        message: format!(
            "capability `{capability}` has {} scenarios titled `{title}`: {sites}",
            places.len()
        ),
        changes: places
            .iter()
            .filter_map(|(_, source)| match source {
                Source::Main => None,
                Source::Change(name) => Some(name.clone()),
            })
            .collect(),
    }
}

/// A claim on a scenario the specs do not have.
fn orphan(claim: &Claim, dropped: &BTreeMap<(String, String), Dropped>) -> Failure {
    let Claim {
        capability,
        scenario,
        function,
        site,
    } = claim;
    let (reason, changes) = match dropped.get(&(capability.clone(), scenario.clone())) {
        Some(Dropped {
            change,
            requirement,
            how: Replacement::Removed,
        }) => (
            format!("but change `{change}` removes its requirement `{requirement}`"),
            vec![change.clone()],
        ),
        Some(Dropped {
            change,
            requirement,
            how: Replacement::Modified,
        }) => (
            format!(
                "but change `{change}` modifies its requirement `{requirement}` and no longer has this scenario"
            ),
            vec![change.clone()],
        ),
        None => ("but no spec or open change has it".to_owned(), Vec::new()),
    };
    Failure {
        site: site.clone(),
        message: format!(
            "`{function}` claims scenario `{scenario}` of capability `{capability}`, {reason}"
        ),
        changes,
    }
}

fn join(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}
