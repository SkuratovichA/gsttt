//! Building an index from the output of the pipeline: the graph, the record of every file and the coverage.

use std::collections::HashSet;
use std::time::Instant;

use foldhash::fast::RandomState;

use super::pipeline::{Output, display_root};
use crate::contract::{ContractKey, Role};
use crate::coverage::{ConnectorCoverage, Coverage, FailedFile};
use crate::fact::{Line, ParseStatus, UnresolvedReason, UnresolvedSite};
use crate::glob::glob_set;
use crate::graph::{ContractSite, GraphBuilder};
use crate::ids::{FileId, Id};
use crate::index::{FileRecord, Index, IndexInfo, ProjectInfo};
use crate::repo::RepoView;
use crate::setup::GraphqlEndpoint;

/// Builds the index of the files of `repo` from what the pipeline found. `ran` names the connectors that took part in registry order, so one that opened no project still has its line in the coverage. The time the build took is added to `info` as the graph phase.
pub fn build_index(
    repo: &RepoView,
    output: &Output,
    ran: &[&str],
    http_prefixes: &[String],
    graphql_endpoints: &[GraphqlEndpoint],
    mut info: IndexInfo,
) -> Index {
    let started = Instant::now();
    let mut graph = GraphBuilder::new(repo.files().map(|(_, path)| path.clone()).collect());
    let mut connectors: Vec<ConnectorCoverage> = ran
        .iter()
        .map(|name| ConnectorCoverage::new(name))
        .collect();

    let mut projects = Vec::with_capacity(output.projects.len());
    let mut slot_of_project = Vec::with_capacity(output.projects.len());
    for project in &output.projects {
        let name = project.connector.name;
        let slot = connectors
            .iter()
            .position(|entry| entry.connector == name)
            .unwrap_or_else(|| {
                connectors.push(ConnectorCoverage::new(name));
                connectors.len() - 1
            });
        if let Some(entry) = connectors.get_mut(slot) {
            entry.projects.push(display_root(&project.root).to_owned());
            entry.provides.extend(project.provides.iter().cloned());
        }
        slot_of_project.push(slot);
        projects.push(ProjectInfo {
            connector: name.to_owned(),
            root: project.root.clone(),
        });
    }

    let mut records: Vec<FileRecord> = repo
        .files()
        .map(|_| FileRecord {
            project: None,
            status: ParseStatus::default(),
            imports: Vec::new(),
            unresolved: Vec::new(),
        })
        .collect();

    // The own facts that the link keyed again, each by the file, line and role of the linked fact that names it. Probed per own fact and never iterated, so the order of a hash set cannot reach the output.
    let replaced: HashSet<(FileId, Line, Role, &ContractKey), RandomState> = output
        .linked
        .iter()
        .filter_map(|linked| {
            Some((
                linked.file,
                linked.fact.line,
                linked.fact.role,
                linked.replaces.as_ref()?,
            ))
        })
        .collect();

    let mut skipped = Vec::new();
    for file in &output.files {
        let Some(entry) = slot_of_project
            .get(file.project.index())
            .and_then(|&slot| connectors.get_mut(slot))
        else {
            continue;
        };
        match &file.facts.status {
            ParseStatus::Failed(failure) if failure.is_skipped() => {
                // The connector never saw this file, so it is listed apart from the files that failed in a parse.
                entry.files = entry.files.saturating_add(1);
                skipped.push(FailedFile {
                    file: file.id,
                    failure: failure.clone(),
                });
            }
            status => entry.count_file(file.id, status),
        }
        graph.tag(file.id, file.facts.tags);
        graph.exports(file.id, file.facts.exports.as_deref());
        graph.symbols(file.id, file.facts.symbols.as_ref());

        for resolved in &file.imports {
            entry.count_import(file.id, resolved);
            for &target in resolved.to.files() {
                graph.import(file.id, target, &resolved.import);
            }
        }
        let mut unresolved = file.facts.unresolved.clone();
        let parsed = unresolved.len();
        unresolved.extend(output.unread_of(file.id).cloned());
        if unresolved.len() > parsed {
            // The parse and the link may report one site, and it is one site however many say so.
            unresolved.sort_by(|a, b| (a.line, &a.detail).cmp(&(b.line, &b.detail)));
            unresolved.dedup_by(|a, b| a.line == b.line && a.detail == b.detail);
        }
        for site in &unresolved {
            entry.count_blind(file.id, site);
        }
        for fact in &file.facts.contracts {
            if replaced.contains(&(file.id, fact.line, fact.role, &fact.key)) {
                // The link keyed this declaration again, once per mount, so the parsed key would be a second site.
                continue;
            }
            let site = ContractSite {
                file: file.id,
                line: fact.line,
                role: fact.role,
                via: fact.via.clone(),
            };
            graph.site(fact.key.clone(), site);
        }
        for (document, read) in file.facts.embedded.iter().zip(&file.embedded) {
            if read.read_by.is_none() {
                // Nobody reads this language, so whatever the document says is invisible to the graph.
                let blind = UnresolvedSite {
                    line: document.line,
                    reason: UnresolvedReason::Unsupported(format!(
                        "embedded {} document",
                        document.lang
                    )),
                    detail: document.via.clone(),
                };
                entry.count_blind(file.id, &blind);
                unresolved.push(blind);
                continue;
            }
            for fact in &read.facts {
                let site = ContractSite {
                    file: file.id,
                    line: fact.line,
                    role: fact.role,
                    via: fact.via.clone(),
                };
                graph.site(fact.key.clone(), site);
            }
        }
        if let Some(record) = records.get_mut(file.id.index()) {
            *record = FileRecord {
                project: Some(file.project),
                status: file.facts.status.clone(),
                imports: file.imports.clone(),
                unresolved,
            };
        }
    }

    for linked in &output.linked {
        let site = ContractSite {
            file: linked.file,
            line: linked.fact.line,
            role: linked.fact.role,
            via: linked.fact.via.clone(),
        };
        graph.site(linked.fact.key.clone(), site);
    }

    let mut coverage = Coverage {
        connectors,
        skipped,
        ..Coverage::default()
    };
    for &id in &output.unclaimed {
        if let Some(path) = repo.path(id) {
            coverage.count_unclaimed(path);
        }
    }
    coverage.problems = output.problems.iter().map(ToString::to_string).collect();
    coverage
        .problems
        .extend(unmatched_endpoint_globs(repo, graphql_endpoints));
    coverage.finish();

    let graph = graph.build();
    info.timings.graph = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    Index {
        graph,
        projects,
        records,
        coverage,
        info,
        http_prefixes: http_prefixes.to_vec(),
        graphql_endpoints: graphql_endpoints.to_vec(),
    }
}

/// One problem line for each glob of an endpoint that matches no file of `repo`: such a glob is valid, so only the coverage can say that the files it meant belong to no endpoint. Done once here, never per query, and not at all while no endpoint lists a glob. The walk of the files stops at the file that matches the last glob still unmatched. A list that does not build is the config check's to report, and gives no line.
fn unmatched_endpoint_globs(repo: &RepoView, endpoints: &[GraphqlEndpoint]) -> Vec<String> {
    let mut globs = Vec::new();
    for (position, endpoint) in endpoints.iter().enumerate() {
        for (side, patterns) in endpoint.sides() {
            for pattern in patterns {
                globs.push((GraphqlEndpoint::place(position, side), pattern));
            }
        }
    }
    if globs.is_empty() {
        return Vec::new();
    }
    let patterns: Vec<String> = globs.iter().map(|(_, glob)| (*glob).clone()).collect();
    let Ok(set) = glob_set(&patterns) else {
        return Vec::new();
    };
    let mut matched = vec![false; globs.len()];
    let mut unmatched = globs.len();
    let mut hits = Vec::new();
    for (_, path) in repo.files() {
        set.matches_into(path.as_str(), &mut hits);
        for &glob in &hits {
            if let Some(seen) = matched.get_mut(glob)
                && !*seen
            {
                *seen = true;
                unmatched = unmatched.saturating_sub(1);
            }
        }
        if unmatched == 0 {
            break;
        }
    }
    globs
        .into_iter()
        .zip(matched)
        .filter(|(_, matched)| !matched)
        .map(|((place, glob), _)| {
            format!(
                "`{place}` has the glob {glob:?}, which matches no indexed file (git-ignored and excluded files are not indexed)"
            )
        })
        .collect()
}
