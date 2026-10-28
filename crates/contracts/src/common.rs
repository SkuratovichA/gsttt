//! Helpers the six contract connectors share.

use gsttt_core::connector::Provide;
use gsttt_core::contract::{ContractKey, ContractKind, Role};
use gsttt_core::fact::{ContractFact, FileFacts, Line, ParseFailure, Resolution, UnresolvedReason};
use gsttt_core::path::RepoPath;
use gsttt_core::repo::RepoView;
use gsttt_core::tree::SyntaxError;

/// A site of a contract of `kind`, with the key put in the canonical form of its kind.
pub(crate) fn site(
    kind: ContractKind,
    key: &str,
    role: Role,
    line: Line,
    via: &str,
) -> ContractFact {
    ContractFact {
        key: ContractKey::new(kind, key),
        role,
        line,
        via: via.to_owned(),
    }
}

/// A site of an HTTP route; the path goes through the shared normaliser, so a server prefix and a parameter style never keep two spellings of one route apart.
pub(crate) fn http_site(
    method: &str,
    path: &str,
    role: Role,
    line: Line,
    via: &str,
) -> ContractFact {
    ContractFact {
        key: ContractKey::http(method, path),
        role,
        line,
        via: via.to_owned(),
    }
}

/// What a connector provides, for coverage.
pub(crate) fn provide(
    kind: ContractKind,
    role: Role,
    via: &[&str],
    blind_spots: &[&str],
) -> Provide {
    Provide {
        kind,
        role,
        via: via.iter().map(|text| (*text).to_owned()).collect(),
        blind_spots: blind_spots.iter().map(|text| (*text).to_owned()).collect(),
    }
}

/// The 1-based line number of the line at `index` in a text.
pub(crate) fn line_number(index: usize) -> Line {
    u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX)
}

/// Whether the repository holds a file that `claims` takes.
pub(crate) fn any_claimed(repo: &RepoView, claims: fn(&RepoPath) -> bool) -> bool {
    repo.files().any(|(_, path)| claims(path))
}

/// Where an import written relative to the importing file points.
pub(crate) fn resolve_relative(repo: &RepoView, from: &RepoPath, spec: &str) -> Resolution {
    RepoPath::join(from.parent(), spec)
        .ok()
        .and_then(|target| repo.lookup(target.as_str()))
        .map_or(Resolution::Unresolved(UnresolvedReason::NotFound), |file| {
            Resolution::File { file }
        })
}

/// Facts of a file the format's parser could not read.
pub(crate) fn syntax_failure(error: SyntaxError) -> FileFacts {
    FileFacts::failed(ParseFailure::Syntax {
        line: error.line,
        message: error.message,
    })
}
