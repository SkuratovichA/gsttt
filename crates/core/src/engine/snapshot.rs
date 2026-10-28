//! The snapshot file: an index and the facts it was built from, on disk.
//!
//! `snapshot.gsttt` starts with the magic `GSTTSNAP`, the format number and the length of the header as two
//! little-endian `u32`, then the header as JSON, then zero padding up to a multiple of 16 bytes, then the body. The
//! header says what the index is without touching the body. The body is rkyv, validated before anything reads it,
//! so a truncated or corrupted file counts as no snapshot instead of taking the process down.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rkyv::rancor::Error as RkyvError;
use rkyv::util::AlignedVec;
use serde::{Deserialize, Serialize};

use super::files::Stamp;
use super::keys::FactKey;
use crate::fact::{FileFacts, ParseFailure, ParseStatus};
use crate::index::Index;
use crate::path::RepoPath;

/// The version of gsttt that writes snapshots. A snapshot another version wrote is not read, because the layout
/// of the body may differ.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const MAGIC: &[u8; 8] = b"GSTTSNAP";
/// The layout of the file around the body. Bump it when the prefix or the header changes shape.
const FORMAT: u32 = 1;
/// Magic, format and header length.
const PREFIX: usize = 16;
/// The body starts at a multiple of this, so the archived values in it sit where rkyv needs them.
const ALIGN: usize = 16;
/// A header is a few hundred bytes; a length far beyond that means the file is not a snapshot.
const MAX_HEADER: usize = 1 << 20;

/// What the first bytes of a snapshot say about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The gsttt version that wrote the file.
    pub built_by: String,
    /// When it was written, as RFC 3339 in UTC.
    pub built_at: String,
    /// The commit HEAD pointed at, `None` outside git or before the first commit.
    pub head: Option<String>,
    /// How many files differed from HEAD.
    pub dirty: u32,
    /// How many files the index covers.
    pub files: u32,
    /// The connectors that ran, in registry order.
    pub connectors: Vec<String>,
    /// The coverage as one line.
    pub coverage: String,
}

/// The body of a snapshot: the index and what a later run needs to skip work.
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Stored {
    /// The index as the run built it.
    pub index: Index,
    /// A hash of everything besides file content that shapes facts and graph: the setup, the connectors, the packs.
    pub config: u128,
    /// One entry per file of the index, in file id order.
    pub files: Vec<StoredFile>,
}

/// A file of a snapshot with the facts a later run can take instead of parsing it again.
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct StoredFile {
    pub path: RepoPath,
    /// The fact key; `None` for a file no project took, and for one whose content id is unknown.
    pub key: Option<FactKey>,
    pub stamp: Stamp,
    pub facts: FileFacts,
    /// The project's link data for the file.
    pub extra: Vec<u8>,
}

/// Why a snapshot could not be written.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("cannot encode the snapshot `{path}`: {message}")]
    Encode { path: String, message: String },
    #[error("cannot write the snapshot `{path}`: {message}")]
    Write { path: String, message: String },
}

/// Writes the snapshot through a temporary file in the same directory, so a reader sees the old file or the new
/// one and never half of either.
pub fn write(path: &Path, header: &Header, stored: &Stored) -> Result<(), SnapshotError> {
    let failed_to_write = |message: String| SnapshotError::Write {
        path: path.display().to_string(),
        message,
    };
    let encode = |message: String| SnapshotError::Encode {
        path: path.display().to_string(),
        message,
    };
    let body = rkyv::to_bytes::<RkyvError>(stored).map_err(|error| encode(error.to_string()))?;
    let json = serde_json::to_vec(header).map_err(|error| encode(error.to_string()))?;
    let length = u32::try_from(json.len()).map_err(|error| encode(error.to_string()))?;

    let mut bytes = Vec::with_capacity(PREFIX + json.len() + ALIGN + body.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&FORMAT.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.resize(bytes.len().next_multiple_of(ALIGN), 0);
    bytes.extend_from_slice(&body);

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|error| failed_to_write(error.to_string()))?;
    let mut file =
        tempfile::NamedTempFile::new_in(dir).map_err(|error| failed_to_write(error.to_string()))?;
    file.write_all(&bytes)
        .map_err(|error| failed_to_write(error.to_string()))?;
    file.persist(path)
        .map_err(|error| failed_to_write(error.to_string()))?;
    Ok(())
}

/// The header of the snapshot at `path`, read without its body. `None` when there is no readable snapshot of this
/// version.
pub fn peek(path: &Path) -> Option<Header> {
    let mut file = fs::File::open(path).ok()?;
    read_header(&mut file).map(|(header, _)| header)
}

/// The header of the snapshot `file` holds, and the offset where the header ends. `None` unless the file starts
/// like a snapshot of this format and version.
fn read_header(file: &mut fs::File) -> Option<(Header, usize)> {
    let mut prefix = [0_u8; PREFIX];
    file.read_exact(&mut prefix).ok()?;
    let length = parse_prefix(&prefix)?;
    let mut json = vec![0_u8; length];
    file.read_exact(&mut json).ok()?;
    Some((parse_header(&json)?, PREFIX + length))
}

/// The snapshot at `path` whole. `None` when the file is missing, from another version, truncated or fails
/// validation: the caller builds the index again.
pub fn read(path: &Path) -> Option<(Header, Stored)> {
    let (header, body) = load(path)?;
    let stored = rkyv::from_bytes::<Stored, RkyvError>(&body).ok()?;
    Some((header, stored))
}

/// The files of the snapshot at `path` whose keys are in `wanted` and whose facts another run may take. The rest
/// of the body is validated but never turned into owned values.
pub fn reusable_facts<S: std::hash::BuildHasher>(
    path: &Path,
    wanted: &HashSet<FactKey, S>,
) -> Vec<(FactKey, StoredFile)> {
    let Some((_, body)) = load(path) else {
        return Vec::new();
    };
    let Ok(archived) = rkyv::access::<ArchivedStored, RkyvError>(&body) else {
        return Vec::new();
    };
    archived
        .files
        .iter()
        .filter_map(|file| {
            let key = file.key.as_ref()?.to_native();
            if !wanted.contains(&key) {
                return None;
            }
            let stored = rkyv::deserialize::<StoredFile, RkyvError>(file).ok()?;
            is_reusable(&stored.facts).then_some((key, stored))
        })
        .collect()
}

/// Whether facts may be copied from one run to another. A parse that panicked or could not read the file says
/// something about that run, not about the content, so it is tried again.
pub fn is_reusable(facts: &FileFacts) -> bool {
    !matches!(
        facts.status,
        ParseStatus::Failed(
            ParseFailure::Panicked { .. }
                | ParseFailure::TooLarge { .. }
                | ParseFailure::Unreadable { .. }
        )
    )
}

/// The header and the body of the snapshot at `path`. The body is read straight to where rkyv can use it, and
/// nothing is read of a file whose header is not this version's.
fn load(path: &Path) -> Option<(Header, AlignedVec<ALIGN>)> {
    let mut file = fs::File::open(path).ok()?;
    let (header, header_end) = read_header(&mut file)?;
    let padding = header_end.next_multiple_of(ALIGN) - header_end;
    file.seek_relative(i64::try_from(padding).ok()?).ok()?;
    let mut body = AlignedVec::<ALIGN>::new();
    body.extend_from_reader(&mut file).ok()?;
    Some((header, body))
}

/// The length of the header when `bytes` start like a snapshot of this format.
fn parse_prefix(bytes: &[u8]) -> Option<usize> {
    let magic = bytes.first_chunk::<8>()?;
    let format = bytes.get(8..)?.first_chunk::<4>()?;
    let length = bytes.get(12..)?.first_chunk::<4>()?;
    if magic != MAGIC || u32::from_le_bytes(*format) != FORMAT {
        return None;
    }
    let length = usize::try_from(u32::from_le_bytes(*length)).ok()?;
    (length <= MAX_HEADER).then_some(length)
}

fn parse_header(json: &[u8]) -> Option<Header> {
    let header: Header = serde_json::from_slice(json).ok()?;
    (header.built_by == VERSION).then_some(header)
}

/// The current time as RFC 3339 in UTC.
pub fn now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    rfc3339(secs)
}

/// Seconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`. Written out here because a calendar crate would be a
/// dependency for this one line of the header.
pub fn rfc3339(secs: u64) -> String {
    let (year, month, day) = civil_from_days(secs / 86_400);
    let rest = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// The date `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(magic: &[u8], format: u32, length: u32) -> Vec<u8> {
        let mut bytes = magic.to_vec();
        bytes.extend(format.to_le_bytes());
        bytes.extend(length.to_le_bytes());
        bytes
    }

    #[test]
    fn a_time_prints_as_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn a_prefix_of_another_file_or_format_is_rejected() {
        assert_eq!(parse_prefix(&prefix(MAGIC, FORMAT, 40)), Some(40));
        assert_eq!(parse_prefix(&prefix(b"NOTASNAP", FORMAT, 40)), None);
        assert_eq!(parse_prefix(&prefix(MAGIC, FORMAT + 1, 40)), None);
        assert_eq!(parse_prefix(&prefix(MAGIC, FORMAT, u32::MAX)), None);
        assert_eq!(parse_prefix(b"GSTT"), None);
    }
}
