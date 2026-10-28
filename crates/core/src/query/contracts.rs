//! `contract`: the sites of one contract by role, or the contract keys that contain some text.

use std::fmt;

use serde::Serialize;

use crate::contract::{ContractKey, Role};
use crate::fact::Line;
use crate::graph::{ContractSite, Graph};
use crate::ids::FileId;
use crate::index::{Freshness, Index};
use crate::text::{noun, thousands};

use super::cross::Crossing;
use super::render::{Answer, Entries, Footer, display_as_text, list};

/// A place in a file that declares, serves or calls a contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Site {
    pub path: String,
    pub line: Line,
    /// The key of the contract the site belongs to, which differs from the key asked for when an HTTP key meets it.
    pub key: String,
    /// The pack or format that found the site.
    pub via: String,
}

/// A contract key that contains the text searched for, with how many sites it has in each role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Match {
    pub key: String,
    pub declares: u32,
    pub serves: u32,
    pub calls: u32,
}

/// What a lookup found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "lookup", rename_all = "kebab-case")]
pub enum Found {
    /// The text names a contract of the index: its sites by role, and for an HTTP key the sites of every key that
    /// meets it.
    Key {
        key: String,
        declares: Vec<Site>,
        serves: Vec<Site>,
        calls: Vec<Site>,
    },
    /// The text is no contract of the index: the keys that contain it, ignoring case, in key order.
    Search { matches: Vec<Match> },
}

/// The answer to a contract lookup.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContractLookup {
    /// The text asked for.
    pub query: String,
    pub found: Found,
    /// How many contracts the index holds, which is what a lookup that finds nothing searched.
    pub contracts: u32,
    pub footer: Footer,
}

/// Why a contract could not be looked up.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContractLookupError {
    /// There was no text to look up. A search for no text would list every contract, which reads as an answer to a
    /// question nobody asked, so the caller is told what to ask for instead.
    #[error(
        "no contract was asked for: expected a key like `http GET /bookings/{{}}`, or part of one"
    )]
    Blank,
}

/// The sites of the contract `query` names, or else the contracts whose keys contain `query`.
///
/// `query` names a contract when it reads as `<kind> <key>` in any spelling that [`ContractKey::parse`] puts in
/// canonical form, and the index has sites for that key, or for an HTTP key that meets it.
///
/// # Errors
///
/// [`ContractLookupError::Blank`] when `query` is empty or only whitespace.
pub fn contract(
    index: &Index,
    freshness: Freshness,
    query: &str,
) -> Result<ContractLookup, ContractLookupError> {
    let graph = &index.graph;
    let query = query.trim();
    if query.is_empty() {
        return Err(ContractLookupError::Blank);
    }
    let key_sites = ContractKey::parse(query)
        .ok()
        .map(|key| (Crossing::new(index).meeting(&key), key))
        .filter(|(meeting, _)| meeting.iter().any(|&id| !graph.sites(id).is_empty()));

    let (found, files) = match key_sites {
        Some((meeting, key)) => {
            let mut declares = Vec::new();
            let mut serves = Vec::new();
            let mut calls = Vec::new();
            let mut files = Vec::new();
            for &id in &meeting {
                let Some(contract_key) = graph.contract(id) else {
                    continue;
                };
                let name = contract_key.to_string();
                for site in graph.sites(id) {
                    let Some(placed) = place(graph, site, &name) else {
                        continue;
                    };
                    files.push(site.file);
                    match site.role {
                        Role::Declares => declares.push(placed),
                        Role::Serves => serves.push(placed),
                        Role::Calls => calls.push(placed),
                    }
                }
            }
            for sites in [&mut declares, &mut serves, &mut calls] {
                sites.sort_by(|a, b| (&a.path, a.line, &a.key).cmp(&(&b.path, b.line, &b.key)));
            }
            let found = Found::Key {
                key: key.to_string(),
                declares,
                serves,
                calls,
            };
            (found, files)
        }
        None => search(graph, query),
    };
    Ok(ContractLookup {
        query: query.to_owned(),
        found,
        contracts: u32::try_from(graph.contract_count()).unwrap_or(u32::MAX),
        footer: Footer::new(index, freshness, files),
    })
}

fn place(graph: &Graph, site: &ContractSite, key: &str) -> Option<Site> {
    Some(Site {
        path: graph.path(site.file)?.to_string(),
        line: site.line,
        key: key.to_owned(),
        via: site.via.clone(),
    })
}

/// The contracts whose key, written `<kind> <key>`, contains `query` ignoring case, with the files of their sites.
fn search(graph: &Graph, query: &str) -> (Found, Vec<FileId>) {
    let needle = query.to_lowercase();
    let mut files = Vec::new();
    let mut matches = Vec::new();
    for (id, key) in graph.contracts() {
        let key = key.to_string();
        if !key.to_lowercase().contains(&needle) {
            continue;
        }
        let sites = graph.sites(id);
        files.extend(sites.iter().map(|site| site.file));
        let count = |role: Role| {
            let sites = sites.iter().filter(|site| site.role == role).count();
            u32::try_from(sites).unwrap_or(u32::MAX)
        };
        matches.push(Match {
            key,
            declares: count(Role::Declares),
            serves: count(Role::Serves),
            calls: count(Role::Calls),
        });
    }
    (Found::Search { matches }, files)
}

impl Answer for ContractLookup {
    fn write_text(&self, f: &mut fmt::Formatter<'_>, entries: Entries) -> fmt::Result {
        match &self.found {
            Found::Key {
                key,
                declares,
                serves,
                calls,
            } => {
                writeln!(f, "{key}")?;
                // The key is worth a word only on the sites that do not have the key asked for.
                let each = |f: &mut fmt::Formatter<'_>, site: &Site| {
                    write!(f, "{}:{}", site.path, site.line)?;
                    if site.key != *key {
                        write!(f, "  {}", site.key)?;
                    }
                    if !site.via.is_empty() {
                        write!(f, "  via {}", site.via)?;
                    }
                    Ok(())
                };
                list(f, "declares", declares, entries, each)?;
                list(f, "serves", serves, entries, each)?;
                list(f, "calls", calls, entries, each)?;
            }
            Found::Search { matches } if matches.is_empty() => writeln!(
                f,
                "no contract key contains `{}` among {}",
                self.query,
                noun(self.contracts, "contract")
            )?,
            Found::Search { matches } => {
                let heading = format!("contracts whose key contains `{}`", self.query);
                list(f, &heading, matches, entries, |f, found| {
                    write!(
                        f,
                        "{}  declares {}, serves {}, calls {}",
                        found.key,
                        thousands(found.declares),
                        thousands(found.serves),
                        thousands(found.calls)
                    )
                })?;
            }
        }
        self.footer.write(f)
    }
}

display_as_text!(ContractLookup);
