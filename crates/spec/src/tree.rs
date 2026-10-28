//! The `openspec/` directory: the main specs and the open changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::capability::{Delta, Spec, parse_delta, parse_spec};
use crate::error::Error;
use crate::tasks::{Task, parse_tasks};

/// Everything under `openspec/` that the spec gate looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSpec {
    /// The main specs, sorted by capability.
    pub specs: Vec<Spec>,
    /// The open changes, sorted by name. The `archive` directory is not a change.
    pub changes: Vec<Change>,
}

/// An open change: `openspec/changes/<name>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The directory name.
    pub name: String,
    /// The `tasks.md` of the change, relative to the repository root, whether or not it exists.
    pub tasks_file: PathBuf,
    /// The checkboxes of `tasks.md`; empty when the file is missing.
    pub tasks: Vec<Task>,
    /// The change's deltas, sorted by capability.
    pub deltas: Vec<Delta>,
}

impl Change {
    /// Whether every task is ticked. A change with no task at all is a proposal that has not been
    /// broken into work yet, so it is not finished and is never archived.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !self.tasks.is_empty() && self.tasks.iter().all(|task| task.done)
    }
}

/// Reads `openspec/specs/**/spec.md` and, for each directory of `openspec/changes/` other than
/// `archive`, its `tasks.md` and `specs/**/spec.md`. `root` is the repository root; the files in
/// the result and in errors are named relative to it.
///
/// # Errors
///
/// Fails when `root` has no `openspec` directory, when a file cannot be read, and with the file
/// and line of the first one that does not follow the format.
pub fn read(root: &Path) -> Result<OpenSpec, Error> {
    let base = Path::new("openspec");
    if !root.join(base).is_dir() {
        let source = io::Error::new(io::ErrorKind::NotFound, "not a directory");
        return Err(Error::Read {
            path: base.to_path_buf(),
            source,
        });
    }
    let mut specs = Vec::new();
    for (capability, file) in find_specs(root, &base.join("specs"))? {
        specs.push(parse_spec(&capability, &file, &read_text(root, &file)?)?);
    }
    let changes_dir = base.join("changes");
    let mut changes = Vec::new();
    for entry in entries(root, &changes_dir)? {
        if entry.is_dir && entry.name != "archive" {
            changes.push(read_change(
                root,
                &changes_dir.join(&entry.name),
                entry.name,
            )?);
        }
    }
    Ok(OpenSpec { specs, changes })
}

fn read_change(root: &Path, dir: &Path, name: String) -> Result<Change, Error> {
    let tasks_file = dir.join("tasks.md");
    let tasks = if root.join(&tasks_file).is_file() {
        parse_tasks(&read_text(root, &tasks_file)?)
    } else {
        Vec::new()
    };
    let mut deltas = Vec::new();
    for (capability, file) in find_specs(root, &dir.join("specs"))? {
        deltas.push(parse_delta(&capability, &file, &read_text(root, &file)?)?);
    }
    Ok(Change {
        name,
        tasks_file,
        tasks,
        deltas,
    })
}

/// Every `spec.md` below `dir` with its capability, sorted by capability. A `spec.md` directly in
/// `dir` has no capability and is left out.
fn find_specs(root: &Path, dir: &Path) -> Result<Vec<(String, PathBuf)>, Error> {
    let mut found = Vec::new();
    collect_specs(root, dir, "", &mut found)?;
    found.sort();
    Ok(found)
}

fn collect_specs(
    root: &Path,
    dir: &Path,
    capability: &str,
    found: &mut Vec<(String, PathBuf)>,
) -> Result<(), Error> {
    for entry in entries(root, dir)? {
        if entry.is_dir {
            let nested = if capability.is_empty() {
                entry.name.clone()
            } else {
                format!("{capability}/{}", entry.name)
            };
            collect_specs(root, &dir.join(&entry.name), &nested, found)?;
        } else if entry.name == "spec.md" && !capability.is_empty() {
            found.push((capability.to_owned(), dir.join("spec.md")));
        }
    }
    Ok(())
}

/// A directory entry, reduced to what the reader needs.
struct Entry {
    name: String,
    is_dir: bool,
}

/// The entries of `dir` sorted by name, so that everything the reader emits has a fixed order. A
/// directory that does not exist has none. A symbolic link is not a directory here, whatever it
/// points to: following one could lead back into the tree and read the same specs again.
fn entries(root: &Path, dir: &Path) -> Result<Vec<Entry>, Error> {
    let read_error = |source: io::Error| Error::Read {
        path: dir.to_path_buf(),
        source,
    };
    let listing = match fs::read_dir(root.join(dir)) {
        Ok(listing) => listing,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(read_error(source)),
    };
    let mut found = Vec::new();
    for item in listing {
        let item = item.map_err(read_error)?;
        let kind = item.file_type().map_err(|source| Error::Read {
            path: dir.join(item.file_name()),
            source,
        })?;
        found.push(Entry {
            name: item.file_name().to_string_lossy().into_owned(),
            is_dir: kind.is_dir(),
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

fn read_text(root: &Path, file: &Path) -> Result<String, Error> {
    fs::read_to_string(root.join(file)).map_err(|source| Error::Read {
        path: file.to_path_buf(),
        source,
    })
}
