//! The snapshot file: an index and the facts it was built from, on disk.
//!
//! `snapshot.gsttt` starts with the magic `GSTTSNAP`, the layout number and the length of the header as two little-endian `u32`, and the xxh3 hash of the body as a little-endian `u64`; then comes the header as JSON, then zero padding up to a multiple of 16 bytes, then the body. The header says what the index is without touching the body. The body is rkyv. It is checked against its hash and then validated before anything reads it, so a truncated or damaged file counts as no snapshot instead of taking the process down or answering wrongly.

use std::fs;
use std::io::{Read, Seek};
use std::path::Path;
use std::time::Duration;

use jiff::Timestamp;
use rkyv::rancor::Error as RkyvError;
use rkyv::util::AlignedVec;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use super::files::{self, Stamp};
use super::keys::FactKey;
use crate::atomic;
use crate::fact::{FileFacts, ParseFailure, ParseStatus};
use crate::index::Index;
use crate::path::RepoPath;

/// The version of gsttt that writes snapshots. A snapshot another version wrote is not read.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const MAGIC: &[u8; 8] = b"GSTTSNAP";
/// The layout of the file: the prefix, the header and the body. Bump it by hand whenever any of them changes shape, or the index in the body comes out of another rule, because a stored index answers as it is. The package version cannot do that, because it stays the same across the commits of a branch in development.
const LAYOUT: u32 = 4;
/// Magic, layout, header length and the hash of the body.
const PREFIX: usize = 24;
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
    /// The unix second at which the listing of the files began. A stamp of a file written within two seconds of it proves nothing to a later run ([`Stamp::is_settled`]). It is the start of the listing and not the time of the write because a file is stamped while the listing runs, and a parse in between can take far longer than the margin.
    pub listed_at: i64,
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

/// The name of the temporary file of a write starts with this prefix, so that [`atomic::is_temporary`] takes nothing else in the directory for one.
const TEMP_PREFIX: &str = "snapshot-";

/// How long a temporary file has to lie untouched before the next write takes it for the leftover of a process that died between creating it and renaming it. A write takes a moment, and nothing locks the directory against two processes that write at once, so a younger file may well belong to one that is still running.
const STALE: Duration = Duration::from_hours(1);

/// Writes the snapshot with [`atomic::write`], so a reader sees the old file or the new one and never half of either. Temporary files that earlier writes left behind are removed first.
pub fn write(path: &Path, header: &Header, stored: &Stored) -> Result<(), SnapshotError> {
    let encode = |message: String| SnapshotError::Encode {
        path: path.display().to_string(),
        message,
    };
    let body = rkyv::to_bytes::<RkyvError>(stored).map_err(|error| encode(error.to_string()))?;
    let json = serde_json::to_vec(header).map_err(|error| encode(error.to_string()))?;
    let length = u32::try_from(json.len()).map_err(|error| encode(error.to_string()))?;

    let mut bytes = Vec::with_capacity(PREFIX + json.len() + ALIGN + body.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&LAYOUT.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&xxh3_64(&body).to_le_bytes());
    bytes.extend_from_slice(&json);
    bytes.resize(bytes.len().next_multiple_of(ALIGN), 0);
    bytes.extend_from_slice(&body);

    remove_stale(path.parent().unwrap_or_else(|| Path::new(".")));
    atomic::write(path, TEMP_PREFIX, &bytes).map_err(|error| SnapshotError::Write {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

/// Removes the temporary files of [`write()`] in `dir` that nobody has touched for [`STALE`].
fn remove_stale(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_temporary = entry
            .file_name()
            .to_str()
            .is_some_and(|name| atomic::is_temporary(name, TEMP_PREFIX));
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok());
        if is_temporary && age.is_some_and(|age| age > STALE) {
            // A file that stays is only disk space.
            drop(fs::remove_file(entry.path()));
        }
    }
}

/// The header of the snapshot at `path`, read without its body. `None` when there is no readable snapshot of this version.
pub fn peek(path: &Path) -> Option<Header> {
    let mut file = fs::File::open(path).ok()?;
    read_header(&mut file).map(|(header, _)| header)
}

/// The header of the snapshot `file` holds, and what its prefix says about the rest. `None` unless the file starts like a snapshot of this layout and version.
fn read_header(file: &mut fs::File) -> Option<(Header, Prefix)> {
    let mut bytes = [0_u8; PREFIX];
    file.read_exact(&mut bytes).ok()?;
    let prefix = parse_prefix(&bytes)?;
    let mut json = vec![0_u8; prefix.header];
    file.read_exact(&mut json).ok()?;
    Some((parse_header(&json)?, prefix))
}

/// A snapshot read into memory, aligned for rkyv, so that its body is looked into where it lies instead of being copied out whole.
#[derive(Debug)]
pub struct Snapshot {
    body: AlignedVec<ALIGN>,
}

impl Snapshot {
    /// The snapshot at `path`. `None` when the file is missing, is not a snapshot of this layout and version (the body of such a file is not read), or has a body that does not match the hash in its prefix.
    pub fn open(path: &Path) -> Option<Self> {
        let mut file = fs::File::open(path).ok()?;
        let (_, prefix) = read_header(&mut file)?;
        let header_end = PREFIX + prefix.header;
        let padding = header_end.next_multiple_of(ALIGN) - header_end;
        file.seek_relative(i64::try_from(padding).ok()?).ok()?;
        let mut body = AlignedVec::<ALIGN>::new();
        body.extend_from_reader(&mut file).ok()?;
        (xxh3_64(&body) == prefix.checksum).then_some(Self { body })
    }

    /// The body as rkyv archived it. Validating it walks all of it, so a caller asks once and keeps the answer. `None` when the body does not validate, for instance because another build wrote it: the caller builds the index again.
    pub fn stored(&self) -> Option<&ArchivedStored> {
        rkyv::access::<ArchivedStored, RkyvError>(&self.body).ok()
    }
}

impl ArchivedStored {
    /// The index, copied out of the archive. The entries of the files stay where they are.
    pub fn to_index(&self) -> Option<Index> {
        rkyv::deserialize::<Index, RkyvError>(&self.index).ok()
    }

    /// The entry of `path`. The entries are in path order, so a binary search finds it.
    pub fn file(&self, path: &RepoPath) -> Option<&ArchivedStoredFile> {
        let at = self
            .files
            .binary_search_by(|file| file.path.as_str().cmp(path.as_str()))
            .ok()?;
        self.files.get(at)
    }

    /// The stamp `path` had when the snapshot was written.
    pub fn stamp_of(&self, path: &RepoPath) -> Option<Stamp> {
        rkyv::deserialize::<Stamp, RkyvError>(&self.file(path)?.stamp).ok()
    }

    /// The facts and the link data filed for `path` under `key`, when another run may take them in place of a parse. The key holds the path, so an entry under the same key is the same file with the same content.
    pub fn reusable(&self, path: &RepoPath, key: FactKey) -> Option<(FileFacts, Vec<u8>)> {
        let file = self.file(path)?;
        if file.key != Some(key) {
            return None;
        }
        let facts = rkyv::deserialize::<FileFacts, RkyvError>(&file.facts).ok()?;
        is_reusable(&facts).then(|| (facts, file.extra.to_vec()))
    }
}

/// Whether facts may be copied from one run to another. A parse that panicked or could not read the file says something about that run, not about the content, so it is tried again.
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

/// What the first bytes of a snapshot say about the rest of the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prefix {
    /// The length of the JSON header that follows the prefix.
    header: usize,
    /// The xxh3 hash the body has to have.
    checksum: u64,
}

/// What the prefix says when `bytes` start like a snapshot of this layout.
fn parse_prefix(bytes: &[u8]) -> Option<Prefix> {
    let magic = bytes.first_chunk::<8>()?;
    let layout = bytes.get(8..)?.first_chunk::<4>()?;
    let length = bytes.get(12..)?.first_chunk::<4>()?;
    let checksum = bytes.get(16..)?.first_chunk::<8>()?;
    if magic != MAGIC || u32::from_le_bytes(*layout) != LAYOUT {
        return None;
    }
    let header = usize::try_from(u32::from_le_bytes(*length)).ok()?;
    (header <= MAX_HEADER).then_some(Prefix {
        header,
        checksum: u64::from_le_bytes(*checksum),
    })
}

fn parse_header(json: &[u8]) -> Option<Header> {
    let header: Header = serde_json::from_slice(json).ok()?;
    (header.built_by == VERSION).then_some(header)
}

/// The current time as RFC 3339 in UTC, to the second.
pub fn now() -> String {
    Timestamp::from_second(files::unix_now())
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::engine::files::blob_id;
    use crate::engine::testing::touch;
    use crate::fact::{Import, ImportKind};

    fn entry(path: &str, key: Option<FactKey>, facts: FileFacts) -> StoredFile {
        StoredFile {
            path: RepoPath::normalize(path).unwrap(),
            key,
            stamp: Stamp {
                id: blob_id(path.as_bytes()),
                size: 7,
                mtime_secs: 1_700_000_000,
                mtime_nanos: 5,
            },
            facts,
            extra: path.as_bytes().to_vec(),
        }
    }

    /// `files`, which have to be in path order, written to a snapshot of their own and opened again. The directory is returned so that it lasts as long as the test does.
    fn stored_in(files: Vec<StoredFile>) -> (tempfile::TempDir, Snapshot) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.gsttt");
        write_files(&path, files);
        (dir, Snapshot::open(&path).unwrap())
    }

    /// Writes a snapshot of `files` to `path`.
    fn write_files(path: &Path, files: Vec<StoredFile>) {
        let header = Header {
            built_by: VERSION.to_owned(),
            built_at: now(),
            head: None,
            dirty: 0,
            files: 0,
            connectors: Vec::new(),
            coverage: String::new(),
        };
        let stored = Stored {
            index: Index::default(),
            config: 9,
            listed_at: 11,
            files,
        };
        write(path, &header, &stored).unwrap();
    }

    fn path(text: &str) -> RepoPath {
        RepoPath::normalize(text).unwrap()
    }

    #[test]
    fn a_write_removes_the_temporary_files_an_earlier_one_left_behind() {
        let dir = tempfile::tempdir().unwrap();
        let long_ago = SystemTime::now() - 2 * STALE;
        let dead = dir.path().join("snapshot-dead.tmp");
        let foreign = dir.path().join("notes.tmp");
        let live = dir.path().join("snapshot-live.tmp");
        for file in [&dead, &foreign, &live] {
            fs::write(file, b"x").unwrap();
        }
        touch(&dead, long_ago);
        touch(&foreign, long_ago);

        write_files(&dir.path().join("snapshot.gsttt"), Vec::new());

        // Only the old temporary file goes: a young one may belong to a write still running, and a file of another name is none of the snapshot's.
        assert!(!dead.exists());
        assert!(live.exists() && foreign.exists());
        assert!(Snapshot::open(&dir.path().join("snapshot.gsttt")).is_some());
    }

    fn prefix(magic: &[u8], layout: u32, length: u32, checksum: u64) -> Vec<u8> {
        let mut bytes = magic.to_vec();
        bytes.extend(layout.to_le_bytes());
        bytes.extend(length.to_le_bytes());
        bytes.extend(checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn the_time_of_a_build_is_rfc3339_in_utc_to_the_second() {
        let built_at = now();
        let time: Timestamp = built_at.parse().unwrap();
        assert_eq!(time.to_string(), built_at);
        assert_eq!(built_at.len(), "1970-01-01T00:00:00Z".len(), "{built_at}");
        assert!(built_at.ends_with('Z'), "{built_at}");
    }

    #[test]
    fn an_entry_is_found_by_its_path() {
        let facts = FileFacts::default;
        let (_dir, snapshot) = stored_in(vec![
            entry("a.txt", None, facts()),
            entry("b/c.txt", None, facts()),
            entry("z.txt", None, facts()),
        ]);
        let stored = snapshot.stored().unwrap();

        for found in ["a.txt", "b/c.txt", "z.txt"] {
            let entry = stored.file(&path(found)).unwrap();
            assert_eq!(entry.path.as_str(), found);
        }
        for absent in ["a", "a.txt0", "b", "b/c", "zz.txt", "0.txt"] {
            assert!(stored.file(&path(absent)).is_none(), "{absent}");
        }
        let expected = Stamp {
            id: blob_id(b"b/c.txt"),
            size: 7,
            mtime_secs: 1_700_000_000,
            mtime_nanos: 5,
        };
        assert_eq!(stored.stamp_of(&path("b/c.txt")), Some(expected));
        assert_eq!(stored.stamp_of(&path("c.txt")), None);
    }

    #[test]
    fn facts_are_taken_under_their_key_and_only_when_a_run_may_reuse_them() {
        let mut parsed = FileFacts::default();
        parsed.imports.push(Import::new("x", ImportKind::Static, 1));
        let panicked = FileFacts::failed(ParseFailure::Panicked {
            message: "boom".to_owned(),
        });
        let (_dir, snapshot) = stored_in(vec![
            entry("a.txt", Some(1), parsed.clone()),
            entry("b.txt", Some(2), panicked),
            entry("c.txt", None, parsed.clone()),
        ]);
        let stored = snapshot.stored().unwrap();

        let (facts, extra) = stored.reusable(&path("a.txt"), 1).unwrap();
        assert_eq!((facts, extra), (parsed, b"a.txt".to_vec()));
        // The key of another file, a failure that belongs to its run, and a file stored without a key.
        assert!(stored.reusable(&path("a.txt"), 2).is_none());
        assert!(stored.reusable(&path("b.txt"), 2).is_none());
        assert!(stored.reusable(&path("c.txt"), 3).is_none());
        assert!(stored.reusable(&path("d.txt"), 1).is_none());
    }

    #[test]
    fn a_body_that_fails_validation_has_no_stored_view() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("snapshot.gsttt");
        let json = serde_json::to_vec(&Header {
            built_by: VERSION.to_owned(),
            built_at: now(),
            head: None,
            dirty: 0,
            files: 0,
            connectors: Vec::new(),
            coverage: String::new(),
        })
        .unwrap();
        // Bytes rkyv cannot read as a snapshot body, behind a prefix that vouches for them.
        let body = [0xa5_u8; 64];
        let length = u32::try_from(json.len()).unwrap();
        let mut bytes = prefix(MAGIC, LAYOUT, length, xxh3_64(&body));
        bytes.extend(json);
        bytes.resize(bytes.len().next_multiple_of(ALIGN), 0);
        bytes.extend(body);
        fs::write(&file, bytes).unwrap();

        let snapshot = Snapshot::open(&file).unwrap();

        assert!(snapshot.stored().is_none());
    }

    #[gsttt_scenario::scenario("indexing", "A flipped byte in the body counts as none")]
    #[test]
    fn a_flipped_byte_in_the_body_counts_as_none() {
        let mut damaged = entry("a.txt", None, FileFacts::default());
        damaged.extra = vec![0x5a; 16];
        let (dir, snapshot) = stored_in(vec![damaged]);
        assert!(snapshot.stored().is_some());
        let file = dir.path().join("snapshot.gsttt");
        let mut bytes = fs::read(&file).unwrap();
        // Connector data is bytes of any value, so no validation can object to a changed one.
        let at = bytes
            .windows(16)
            .position(|window| window == [0x5a; 16])
            .unwrap();
        bytes[at + 8] = 0x5b;
        fs::write(&file, bytes).unwrap();

        assert!(Snapshot::open(&file).is_none());
    }

    #[test]
    fn a_prefix_of_another_file_or_layout_is_rejected() {
        let expected = Prefix {
            header: 40,
            checksum: 7,
        };
        assert_eq!(parse_prefix(&prefix(MAGIC, LAYOUT, 40, 7)), Some(expected));
        assert_eq!(parse_prefix(&prefix(b"NOTASNAP", LAYOUT, 40, 7)), None);
        assert_eq!(parse_prefix(&prefix(MAGIC, LAYOUT + 1, 40, 7)), None);
        assert_eq!(parse_prefix(&prefix(MAGIC, LAYOUT, u32::MAX, 7)), None);
        // The prefix of the layout before the checksum was added.
        assert_eq!(parse_prefix(&prefix(MAGIC, 1, 40, 7)), None);
        // The layout whose index holds the parsed and the linked site of a declaration a link re-keys.
        assert_eq!(parse_prefix(&prefix(MAGIC, 2, 40, 7)), None);
        // The layout whose index lacks the site of every own fact that shares a line, a role and a kind with a linked fact, though the link keyed none of them again.
        assert_eq!(parse_prefix(&prefix(MAGIC, 3, 40, 7)), None);
        assert_eq!(parse_prefix(&prefix(MAGIC, LAYOUT, 40, 7)[..20]), None);
        assert_eq!(parse_prefix(b"GSTT"), None);
    }
}
