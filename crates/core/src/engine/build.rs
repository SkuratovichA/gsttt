//! Building an index from the output of the pipeline: the graph, the record of every file and the coverage.

use std::time::Instant;

use super::pipeline::{Output, display_root};
use crate::coverage::{ConnectorCoverage, Coverage, FailedFile};
use crate::fact::{ParseFailure, ParseStatus, UnresolvedReason, UnresolvedSite};
use crate::graph::{ContractSite, Edge, GraphBuilder};
use crate::ids::Id;
use crate::index::{FileRecord, Index, IndexInfo, ProjectInfo};
use crate::repo::RepoView;

/// Builds the index of the files of `repo` from what the pipeline found. `ran` names the connectors that took part
/// in registry order, so one that opened no project still has its line in the coverage. The time the build took is
/// added to `info` as the graph phase.
pub fn build_index(
    repo: &RepoView,
    output: &Output,
    ran: &[&str],
    http_prefixes: &[String],
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

    let mut skipped = Vec::new();
    for file in &output.files {
        let Some(entry) = slot_of_project
            .get(file.project.index())
            .and_then(|&slot| connectors.get_mut(slot))
        else {
            continue;
        };
        match &file.facts.status {
            ParseStatus::Failed(
                failure @ (ParseFailure::TooLarge { .. }
                | ParseFailure::NotUtf8
                | ParseFailure::Unreadable { .. }),
            ) => {
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

        for resolved in &file.imports {
            entry.count_import(file.id, resolved);
            for &target in resolved.to.files() {
                let edge = Edge::new(resolved.import.kind, resolved.import.flags);
                graph.edge(file.id, target, edge);
            }
        }
        let mut unresolved = file.facts.unresolved.clone();
        for site in &file.facts.unresolved {
            entry.count_blind(file.id, site);
        }
        for fact in &file.facts.contracts {
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
    }
}
