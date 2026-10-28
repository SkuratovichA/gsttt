//! The rules of the gate.
//!
//! A scenario needs a proof when the specs will have it once the open changes are archived. The gate finds those scenarios by folding: it starts from the requirements of the main specs and applies the deltas of the open changes to them, the changes in the order that `--ready` would archive them, a finished change before an unfinished one where that order leaves a choice, and each delta the way `openspec archive` applies it. Reading the specs that way keeps a modified scenario from counting twice, and makes the tests follow a change as it lands: once a change removes a scenario, whether the main specs or an open change archived before it wrote the scenario, no test may keep claiming it. The fold also shows what `openspec archive` would refuse: a modified requirement replaces the whole block, and the archive stops at one that lacks a scenario of the block it replaces, so the gate fails that change before the archive does and leaves its block out of the fold, where the requirement it would replace stands. A rename onto a name that stands is refused the same way and not applied.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gsttt_spec::{Change, Delta, OpenSpec, Renamed, Requirement, Scenario};

use super::proofs::{Claim, Marks, Site};
use super::purpose;

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

/// How far an open change has come, by its `tasks.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Progress {
    /// No task at all: a proposal that is not yet broken into work, and so never finished.
    NoTasks,
    /// At least one task is not ticked.
    Unticked,
    /// Every task is ticked.
    Finished,
}

impl Progress {
    fn of(change: &Change) -> Self {
        if change.is_finished() {
            Self::Finished
        } else if change.tasks.is_empty() {
            Self::NoTasks
        } else {
            Self::Unticked
        }
    }
}

/// A requirement or a scenario of a capability, by capability and name or title.
type Key = (String, String);

/// An open change, as far as archiving cares.
#[derive(Debug)]
struct Open {
    name: String,
    progress: Progress,
    /// The requirements that the change modifies, removes or renames: `openspec archive` stops at one that is missing.
    needs: Vec<Key>,
    /// The requirements that the change adds or renames to, which the changes archived after it can then modify, remove or rename.
    gives: Vec<Key>,
    /// The requirements that the change removes or renames away, which the changes archived after it no longer find, each with the entry of the delta that says so.
    takes: Vec<(Key, Site)>,
}

impl Open {
    fn new(change: &Change) -> Self {
        Self {
            name: change.name.clone(),
            progress: Progress::of(change),
            needs: change.deltas.iter().flat_map(needs).collect(),
            gives: change.deltas.iter().flat_map(gives).collect(),
            takes: change.deltas.iter().flat_map(takes).collect(),
        }
    }
}

/// Everything the gate found out.
#[derive(Debug)]
pub(super) struct Judgement {
    /// Sorted by capability, then main scenarios before changes, then by place.
    pub(super) entries: Vec<Entry>,
    /// Sorted by place.
    pub(super) failures: Vec<Failure>,
    changes: Vec<Open>,
    /// The requirements of the main specs.
    main: BTreeSet<Key>,
}

impl Judgement {
    /// The gate passes exactly when nothing failed.
    pub(super) fn passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// How far the open change with this name has come.
    pub(super) fn progress(&self, change: &str) -> Option<Progress> {
        self.changes
            .iter()
            .find(|open| open.name == change)
            .map(|open| open.progress)
    }

    /// The open changes that can be archived, in the order of [`archive_order`]: every task ticked, every scenario proven, and no failure that mentions them.
    pub(super) fn ready(&self) -> Vec<&str> {
        let waiting = self
            .changes
            .iter()
            .filter(|change| change.progress == Progress::Finished)
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
            .collect();
        archive_order(&self.main, waiting)
            .into_iter()
            .map(|change| change.name.as_str())
            .collect()
    }
}

/// How an open change takes a requirement away from the specs and the changes archived before it.
#[derive(Debug, Clone, Copy)]
enum Replacement {
    Removed,
    Modified,
}

/// A scenario that an open change takes away from the main specs or from an open change archived before it.
#[derive(Debug)]
struct Dropped {
    change: String,
    requirement: String,
    how: Replacement,
}

/// Judges the scenarios of the specs against the claims found in the sources.
pub(super) fn judge(openspec: &OpenSpec, marks: &Marks) -> Judgement {
    let changes: Vec<Open> = openspec.changes.iter().map(Open::new).collect();
    let Effective {
        mut entries,
        dropped,
        refusals,
        main,
    } = effective(openspec, &changes);

    let mut homes: BTreeMap<Key, Vec<(Site, Source)>> = BTreeMap::new();
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
            status(entry, &changes, &mut failures)
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
    failures.extend(purpose::failures(openspec));
    failures.extend(rivals(&changes));
    failures.extend(refusals);

    entries.sort_by(|a, b| {
        (&a.capability, &a.source, &a.site).cmp(&(&b.capability, &b.source, &b.site))
    });
    failures.sort();
    Judgement {
        entries,
        failures,
        changes,
        main,
    }
}

/// What the main specs and the open changes make of the scenarios and the requirements.
struct Effective {
    /// The scenarios that need a proof.
    entries: Vec<Entry>,
    /// The scenarios that open changes take away, each with the last change that does.
    dropped: BTreeMap<Key, Dropped>,
    /// The modified requirements that lack a scenario of the requirement they replace and the renames onto a name that stands, which `openspec archive` would refuse.
    refusals: Vec<Failure>,
    /// The requirements of the main specs.
    main: BTreeSet<Key>,
}

/// A requirement that stands once the changes read so far are archived, with the place that wrote it.
struct Held<'a> {
    source: Source,
    file: &'a Path,
    requirement: &'a Requirement,
}

/// The requirements that stand once the changes read so far are archived, by capability and name, the scenarios that those changes took away, and the modified requirements that `openspec archive` would refuse. A name holds more than one requirement only where two specs or two changes add it, which `openspec archive` takes only when the two are the same block: both stay, so that the gate reports the scenarios they share.
struct Fold<'a> {
    standing: BTreeMap<Key, Vec<Held<'a>>>,
    dropped: BTreeMap<Key, Dropped>,
    refusals: Vec<Failure>,
}

impl<'a> Fold<'a> {
    /// Applies a delta of `change` the way `openspec archive` does: renames first, then removals, then modifications, then additions.
    fn apply(&mut self, change: &'a Change, delta: &'a Delta) {
        let capability = &delta.capability;
        let key = |name: &str| (capability.clone(), name.to_owned());
        let written = |requirement: &'a Requirement| Held {
            source: Source::Change(change.name.clone()),
            file: &delta.file,
            requirement,
        };
        for renamed in &delta.renamed {
            let (from, to) = (key(&renamed.from), key(&renamed.to));
            if !self.standing.contains_key(&from) {
                continue;
            }
            // The archive stops at a rename onto a name that stands, so the rename is not applied and both requirements stay as they are.
            if self.standing.contains_key(&to) {
                self.refusals.push(rename_refusal(change, delta, renamed));
            } else if let Some(moved) = self.standing.remove(&from) {
                self.standing.insert(to, moved);
            }
        }
        for removed in &delta.removed {
            let gone = self.standing.remove(&key(&removed.name));
            self.take_away(gone, capability, &change.name, Replacement::Removed);
        }
        for requirement in &delta.modified {
            // Two places that add one name hold one block for the archive, or it stops at the second, so the first stands for them.
            let missing = self
                .standing
                .get(&key(&requirement.name))
                .and_then(|group| group.first())
                .map(|replaced| missing_titles(replaced.requirement, requirement))
                .unwrap_or_default();
            // The archive stops at a block that leaves a scenario out, so the block replaces nothing: what stands keeps standing with its scenarios and the change that wrote them, and the scenarios of the block count for nothing.
            if !missing.is_empty() {
                self.refusals
                    .push(refusal(change, delta, requirement, &missing));
                continue;
            }
            let gone = self
                .standing
                .insert(key(&requirement.name), vec![written(requirement)]);
            self.take_away(gone, capability, &change.name, Replacement::Modified);
        }
        for requirement in &delta.added {
            self.standing
                .entry(key(&requirement.name))
                .or_default()
                .push(written(requirement));
        }
    }

    /// Notes every scenario of the requirements `gone` as taken away by `change`.
    fn take_away(
        &mut self,
        gone: Option<Vec<Held<'_>>>,
        capability: &str,
        change: &str,
        how: Replacement,
    ) {
        for held in gone.into_iter().flatten() {
            for scenario in &held.requirement.scenarios {
                self.dropped.insert(
                    (capability.to_owned(), scenario.title.clone()),
                    Dropped {
                        change: change.to_owned(),
                        requirement: held.requirement.name.clone(),
                        how,
                    },
                );
            }
        }
    }
}

/// Folds every open change, finished or not, over the requirements of the main specs: in the order that [`archive_order`] gives, then the changes it cannot place in the order of their names, each as far as its deltas reach. The scenarios of what stands at the end need a proof.
fn effective(openspec: &OpenSpec, changes: &[Open]) -> Effective {
    let mut fold = Fold {
        standing: BTreeMap::new(),
        dropped: BTreeMap::new(),
        refusals: Vec::new(),
    };
    for spec in &openspec.specs {
        for requirement in &spec.requirements {
            fold.standing
                .entry((spec.capability.clone(), requirement.name.clone()))
                .or_default()
                .push(Held {
                    source: Source::Main,
                    file: &spec.file,
                    requirement,
                });
        }
    }
    let main: BTreeSet<Key> = fold.standing.keys().cloned().collect();
    let placed = archive_order(&main, changes.iter().collect());
    let mut ordered: Vec<&Change> = openspec.changes.iter().collect();
    ordered.sort_by_key(|change| {
        placed
            .iter()
            .position(|open| open.name == change.name)
            .unwrap_or(placed.len())
    });
    for change in ordered {
        for delta in &change.deltas {
            fold.apply(change, delta);
        }
    }
    let mut entries = Vec::new();
    for ((capability, _), group) in &fold.standing {
        for held in group {
            entries.extend(
                held.requirement.scenarios.iter().map(|scenario| {
                    Entry::new(capability, scenario, held.source.clone(), held.file)
                }),
            );
        }
    }
    Effective {
        entries,
        dropped: fold.dropped,
        refusals: fold.refusals,
        main,
    }
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

/// The order in which `openspec archive` can take the changes in `waiting` one after the other, starting from the requirements `main` holds: a change goes next when every requirement it needs is in what `main` and the changes before it leave, and no other waiting change needs a requirement it takes away; the requirements it takes leave before the ones it gives enter. Of the changes that can go next, the first finished one goes first, because the archive takes only finished changes, and when none is finished the first of `waiting`; changes given in the order of their names therefore come out in it where nothing else decides. A change that cannot go, because what it needs is nowhere or because it and another wait for each other, is left out.
fn archive_order<'a>(main: &BTreeSet<Key>, mut waiting: Vec<&'a Open>) -> Vec<&'a Open> {
    let mut have: BTreeSet<&Key> = main.iter().collect();
    let mut listed = Vec::new();
    while let Some((next, _)) = waiting
        .iter()
        .enumerate()
        .filter(|(_, change)| can_go(change, &have, &waiting))
        .min_by_key(|(index, change)| (change.progress != Progress::Finished, *index))
    {
        let change = waiting.remove(next);
        for (key, _) in &change.takes {
            have.remove(key);
        }
        have.extend(&change.gives);
        listed.push(change);
    }
    listed
}

/// Whether `change` can be archived next, when `have` stands and the changes in `waiting` are not archived yet: every requirement it needs stands, and no other waiting change needs a requirement it takes away.
fn can_go(change: &Open, have: &BTreeSet<&Key>, waiting: &[&Open]) -> bool {
    change.needs.iter().all(|need| have.contains(need))
        && change.takes.iter().all(|(key, _)| {
            waiting
                .iter()
                .all(|other| other.name == change.name || !other.needs.contains(key))
        })
}

/// The requirements that `delta` modifies, removes or renames, under the names they have before the change is archived.
fn needs(delta: &Delta) -> impl Iterator<Item = Key> {
    let modified = delta.modified.iter().map(|requirement| {
        (
            delta.capability.clone(),
            main_name(delta, requirement).clone(),
        )
    });
    takes(delta).map(|(key, _)| key).chain(modified)
}

/// The requirements that `delta` removes and the names it renames requirements away from, each with the line of the entry that says so.
fn takes(delta: &Delta) -> impl Iterator<Item = (Key, Site)> {
    let removed = delta
        .removed
        .iter()
        .map(|removed| (&removed.name, removed.line));
    let renamed = delta
        .renamed
        .iter()
        .map(|renamed| (&renamed.from, renamed.line));
    removed.chain(renamed).map(|(name, line)| {
        let site = Site {
            file: delta.file.clone(),
            line,
        };
        ((delta.capability.clone(), name.clone()), site)
    })
}

/// The requirements that `delta` adds, and the names it renames requirements to.
fn gives(delta: &Delta) -> impl Iterator<Item = Key> {
    let added = delta.added.iter().map(|requirement| &requirement.name);
    let renamed = delta.renamed.iter().map(|renamed| &renamed.to);
    added
        .chain(renamed)
        .map(|name| (delta.capability.clone(), name.clone()))
}

/// The status of a scenario that is written once, with the failure when it fails.
fn status(entry: &Entry, open: &[Open], failures: &mut Vec<Failure>) -> Status {
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
    let unfinished = |name: &str| {
        open.iter()
            .any(|change| change.name == name && change.progress != Progress::Finished)
    };
    match (entry.proofs.len(), entry.change()) {
        (1, _) => Status::Proven,
        (0, Some(name)) if unfinished(name) => Status::Pending,
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

/// A failure for each requirement that more than one open change removes or renames away: whichever is archived second would not find it, and `ready` would make each of them wait for the other.
fn rivals(changes: &[Open]) -> Vec<Failure> {
    let mut takers: BTreeMap<&Key, BTreeMap<&str, &Site>> = BTreeMap::new();
    for change in changes {
        for (key, site) in &change.takes {
            takers
                .entry(key)
                .or_default()
                .entry(change.name.as_str())
                .or_insert(site);
        }
    }
    takers
        .into_iter()
        .filter(|(_, by_change)| by_change.len() > 1)
        .map(|((capability, requirement), by_change)| Failure {
            site: by_change
                .values()
                .next()
                .map(|site| (*site).clone())
                .unwrap_or_default(),
            message: format!(
                "requirement `{requirement}` of capability `{capability}` is removed or renamed away by changes {}, and only the first of them to be archived finds it: {}",
                join(by_change.keys().map(|name| format!("`{name}`"))),
                join(by_change.values().map(ToString::to_string))
            ),
            changes: by_change.keys().map(|name| (*name).to_owned()).collect(),
        })
        .collect()
}

/// The titles of the scenarios of `replaced` that `modified` does not match, each once, counting a title as often as it is written the way `openspec archive` does: a title that `replaced` holds more often than `modified` leaves the difference.
fn missing_titles<'a>(replaced: &'a Requirement, modified: &Requirement) -> Vec<&'a str> {
    let mut available: BTreeMap<&str, usize> = BTreeMap::new();
    for scenario in &modified.scenarios {
        *available.entry(scenario.title.as_str()).or_default() += 1;
    }
    let mut missing = Vec::new();
    for scenario in &replaced.scenarios {
        let title = scenario.title.as_str();
        match available.get_mut(title).filter(|left| **left > 0) {
            Some(left) => *left -= 1,
            None if missing.contains(&title) => {}
            None => missing.push(title),
        }
    }
    missing
}

/// A modified requirement that leaves out scenarios of the requirement it replaces, which `openspec archive` refuses: the change waits until it keeps them.
fn refusal(change: &Change, delta: &Delta, modified: &Requirement, missing: &[&str]) -> Failure {
    let titles = join(missing.iter().map(|title| format!("`{title}`")));
    let noun = if missing.len() == 1 {
        "scenario"
    } else {
        "scenarios"
    };
    Failure {
        site: Site {
            file: delta.file.clone(),
            line: modified.line,
        },
        message: format!(
            "change `{}` modifies requirement `{}` of capability `{}` and leaves out {noun} {titles} of the requirement it replaces, and `openspec archive` refuses such a block",
            change.name, modified.name, delta.capability
        ),
        changes: vec![change.name.clone()],
    }
}

/// A rename onto a name that stands, which `openspec archive` refuses: the change waits until it renames to a name that nothing has.
fn rename_refusal(change: &Change, delta: &Delta, renamed: &Renamed) -> Failure {
    Failure {
        site: Site {
            file: delta.file.clone(),
            line: renamed.line,
        },
        message: format!(
            "change `{}` renames requirement `{}` of capability `{}` to `{}`, which already stands, and `openspec archive` refuses such a rename",
            change.name, renamed.from, delta.capability, renamed.to
        ),
        changes: vec![change.name.clone()],
    }
}

/// A claim on a scenario the specs do not have.
fn orphan(claim: &Claim, dropped: &BTreeMap<Key, Dropped>) -> Failure {
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
