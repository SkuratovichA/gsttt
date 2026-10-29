//! The two indexes the runs read, kept aside and put back: the index of gsttt in the `.git/gsttt` of each checkout, and the cache directory of codebase-memory-mcp. A run of their setup can change them (`gsttt index`, or the tools of codebase-memory-mcp that index a directory and delete a project), and every run after it would read what it left.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{ErrorKind, Read as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};

use super::setups::Setup;
use crate::bench::plan::GSTTT_CACHE;

/// What the server of codebase-memory-mcp writes into its cache on its own, found with `diff -r` on a cache before and after a server that only answered a search: the log of its activity with the files its database keeps beside it, and its logs. None of it is the agent's doing, so it is neither compared nor put back.
const CBM_OWN: [&str; 4] = ["activity.db", "activity.db-shm", "activity.db-wal", "logs"];

/// The size of the blocks two files are compared in, so that the database of a large repository is never held whole.
const BLOCK: u64 = 1 << 20;

/// One index as it was when the runs began.
#[derive(Debug)]
struct Aside {
    /// Where the index is.
    live: PathBuf,
    /// Where the copy of it is.
    copy: PathBuf,
    /// The names at the top of `live` that its tool writes on its own, which the copy does not hold.
    own: &'static [&'static str],
}

impl Aside {
    /// Copies `live` to `copy`, in place of an earlier copy.
    fn keep(live: &Path, copy: &Path, own: &'static [&'static str]) -> anyhow::Result<Self> {
        ensure!(
            live.is_dir(),
            "{} is not a directory, so there is no index to keep",
            live.display()
        );
        if copy.exists() {
            fs::remove_dir_all(copy)
                .with_context(|| format!("cannot remove {}", copy.display()))?;
        }
        copy_tree(live, copy, own)?;
        Ok(Self {
            live: live.to_path_buf(),
            copy: copy.to_path_buf(),
            own,
        })
    }

    /// Whether the index holds the same files with the same bytes as the copy.
    fn unchanged(&self) -> anyhow::Result<bool> {
        let live = listing(&self.live, self.own)?;
        if live != listing(&self.copy, &[])? {
            return Ok(false);
        }
        for (path, is_dir) in &live {
            if !is_dir && !same_bytes(&self.live.join(path), &self.copy.join(path))? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Puts the index back as the copy is, and says whether it was not. The names its tool writes on its own are left as they are.
    fn restore(&self) -> anyhow::Result<bool> {
        if self.unchanged()? {
            return Ok(false);
        }
        for entry in children(&self.live)? {
            if is_own(&entry.file_name(), self.own) {
                continue;
            }
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .with_context(|| format!("cannot look at {}", path.display()))?
                .is_dir();
            let removed = if is_dir {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            removed.with_context(|| format!("cannot remove {}", path.display()))?;
        }
        copy_tree(&self.copy, &self.live, &[])?;
        Ok(true)
    }
}

/// The indexes of the checkouts and of codebase-memory-mcp, as they were when the runs began.
#[derive(Debug, Default)]
pub(super) struct Indexes {
    /// The index of gsttt of each checkout, by the name of its repository.
    gsttt: BTreeMap<String, Aside>,
    /// The cache of codebase-memory-mcp, which holds the index of every checkout.
    cbm: Option<Aside>,
}

impl Indexes {
    /// Keeps the index of gsttt that the checkout `dir` of `repository` has now, in `copy`.
    pub(super) fn keep_gsttt(
        &mut self,
        repository: &str,
        dir: &Path,
        copy: &Path,
    ) -> anyhow::Result<()> {
        let aside = Aside::keep(&dir.join(GSTTT_CACHE), copy, &[])?;
        self.gsttt.insert(repository.to_owned(), aside);
        Ok(())
    }

    /// Keeps the cache of codebase-memory-mcp as it is now, in `copy`.
    pub(super) fn keep_cbm(&mut self, cache: &Path, copy: &Path) -> anyhow::Result<()> {
        self.cbm = Some(Aside::keep(cache, copy, &CBM_OWN)?);
        Ok(())
    }

    /// Puts back the index that a run of `setup` in `repository` could have changed, and says whether it had: the index of gsttt for the gsttt setup, the cache of codebase-memory-mcp for the cbm setup, and none for the others.
    pub(super) fn restore(&self, setup: Setup, repository: &str) -> anyhow::Result<bool> {
        let aside = match setup {
            Setup::Gsttt => self.gsttt.get(repository),
            Setup::Cbm => self.cbm.as_ref(),
            Setup::Plain | Setup::Lsp => None,
        };
        aside.map_or(Ok(false), Aside::restore)
    }
}

fn is_own(name: &OsStr, own: &[&str]) -> bool {
    own.iter().any(|own| OsStr::new(own) == name)
}

/// The entries of `dir`; none when it is not there.
fn children(dir: &Path) -> anyhow::Result<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .collect::<Result<_, _>>()
            .with_context(|| format!("cannot read {}", dir.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).with_context(|| format!("cannot read {}", dir.display())),
    }
}

/// Every file and directory below `dir` by its path from `dir`, and whether it is a directory, apart from the names in `own` at the top. A `dir` that is not there holds none.
fn listing(dir: &Path, own: &[&str]) -> anyhow::Result<BTreeMap<PathBuf, bool>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(parent) = pending.pop() {
        for entry in children(&dir.join(&parent))? {
            let name = entry.file_name();
            if parent.as_os_str().is_empty() && is_own(&name, own) {
                continue;
            }
            let path = parent.join(&name);
            let is_dir = entry
                .file_type()
                .with_context(|| format!("cannot look at {}", entry.path().display()))?
                .is_dir();
            if is_dir {
                pending.push(path.clone());
            }
            found.insert(path, is_dir);
        }
    }
    Ok(found)
}

/// Copies `from` to `to` with everything in it, apart from the names in `own` at the top of `from`.
fn copy_tree(from: &Path, to: &Path, own: &[&str]) -> anyhow::Result<()> {
    fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;
    // A directory sorts before what is in it.
    for (path, is_dir) in listing(from, own)? {
        let target = to.join(&path);
        if is_dir {
            fs::create_dir_all(&target)
                .with_context(|| format!("cannot create {}", target.display()))?;
        } else {
            let source = from.join(&path);
            fs::copy(&source, &target).with_context(|| {
                format!("cannot copy {} to {}", source.display(), target.display())
            })?;
        }
    }
    Ok(())
}

/// Whether two files hold the same bytes, read a block at a time.
fn same_bytes(first: &Path, second: &Path) -> anyhow::Result<bool> {
    let open =
        |file: &Path| File::open(file).with_context(|| format!("cannot read {}", file.display()));
    let (mut first_file, mut second_file) = (open(first)?, open(second)?);
    let (mut a, mut b) = (Vec::new(), Vec::new());
    loop {
        a.clear();
        b.clear();
        first_file
            .by_ref()
            .take(BLOCK)
            .read_to_end(&mut a)
            .with_context(|| format!("cannot read {}", first.display()))?;
        second_file
            .by_ref()
            .take(BLOCK)
            .read_to_end(&mut b)
            .with_context(|| format!("cannot read {}", second.display()))?;
        if a != b {
            return Ok(false);
        }
        if a.is_empty() {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn put(dir: &Path, name: &str, text: &str) {
        let file = dir.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).unwrap()
    }

    /// The index of gsttt of a checkout called `repo` and a cache of codebase-memory-mcp, both kept aside.
    struct Bed {
        dir: TempDir,
        indexes: Indexes,
    }

    impl Bed {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let checkout = dir.path().join("checkout");
            put(&checkout.join(GSTTT_CACHE), "snapshot.gsttt", "snapshot\n");
            let cache = dir.path().join("cache");
            put(&cache, "_config.db", "config\n");
            put(&cache, "project.db", "graph\n");
            put(&cache, "activity.db", "activity\n");
            put(&cache, "logs/cbm-daemon.log", "started\n");
            let mut indexes = Indexes::default();
            indexes
                .keep_gsttt("repo", &checkout, &dir.path().join("aside/gsttt"))
                .unwrap();
            indexes
                .keep_cbm(&cache, &dir.path().join("aside/cbm"))
                .unwrap();
            Self { dir, indexes }
        }

        fn gsttt(&self) -> PathBuf {
            self.dir.path().join("checkout").join(GSTTT_CACHE)
        }

        fn cache(&self) -> PathBuf {
            self.dir.path().join("cache")
        }
    }

    #[test]
    fn an_index_that_nothing_touched_is_not_marked() {
        let bed = Bed::new();
        for setup in Setup::ALL {
            assert!(!bed.indexes.restore(setup, "repo").unwrap(), "{setup}");
        }
    }

    #[test]
    fn an_index_of_gsttt_that_a_run_changed_is_put_back_and_marked() {
        let bed = Bed::new();
        let gsttt = bed.gsttt();
        // Other bytes, and a directory with a file in it that was not there.
        put(&gsttt, "snapshot.gsttt", "snapshot of the run\n");
        put(&gsttt, "extra/new.gsttt", "new\n");
        assert!(bed.indexes.restore(Setup::Gsttt, "repo").unwrap());
        assert_eq!(read(&gsttt, "snapshot.gsttt"), "snapshot\n");
        assert!(!gsttt.join("extra").exists());
        assert!(
            !bed.indexes.restore(Setup::Gsttt, "repo").unwrap(),
            "it is put back once"
        );

        fs::remove_file(gsttt.join("snapshot.gsttt")).unwrap();
        assert!(bed.indexes.restore(Setup::Gsttt, "repo").unwrap());
        assert_eq!(read(&gsttt, "snapshot.gsttt"), "snapshot\n");

        fs::remove_dir_all(&gsttt).unwrap();
        assert!(bed.indexes.restore(Setup::Gsttt, "repo").unwrap());
        assert_eq!(read(&gsttt, "snapshot.gsttt"), "snapshot\n");
    }

    #[test]
    fn the_cache_of_codebase_memory_that_a_run_changed_is_put_back_and_marked() {
        let bed = Bed::new();
        let cache = bed.cache();
        fs::remove_file(cache.join("project.db")).unwrap();
        put(&cache, "other.db", "indexed by the run\n");
        put(&cache, "_config.db", "config of the run\n");
        // What the server writes on its own stays as it is.
        put(&cache, "activity.db", "activity of the run\n");
        assert!(bed.indexes.restore(Setup::Cbm, "repo").unwrap());
        assert_eq!(read(&cache, "project.db"), "graph\n");
        assert_eq!(read(&cache, "_config.db"), "config\n");
        assert!(!cache.join("other.db").exists());
        assert_eq!(read(&cache, "activity.db"), "activity of the run\n");
        assert!(!bed.indexes.restore(Setup::Cbm, "repo").unwrap());
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "What a tool writes into its index on its own is not a change"
    )]
    #[test]
    fn what_the_server_of_codebase_memory_writes_on_its_own_is_not_a_change() {
        let bed = Bed::new();
        let cache = bed.cache();
        put(&cache, "activity.db", "activity of the server\n");
        put(&cache, "activity.db-wal", "log\n");
        put(&cache, "activity.db-shm", "memory\n");
        put(
            &cache,
            "logs/cbm-daemon.log",
            "started\nanswered a search\n",
        );
        put(&cache, "logs/project-1.log", "indexed\n");
        assert!(!bed.indexes.restore(Setup::Cbm, "repo").unwrap());
        assert_eq!(read(&cache, "activity.db-wal"), "log\n");
    }

    #[test]
    fn only_the_setup_of_an_index_puts_it_back() {
        let bed = Bed::new();
        put(&bed.gsttt(), "snapshot.gsttt", "changed\n");
        put(&bed.cache(), "project.db", "changed\n");
        for setup in [Setup::Plain, Setup::Lsp] {
            assert!(!bed.indexes.restore(setup, "repo").unwrap(), "{setup}");
        }
        assert!(
            !bed.indexes.restore(Setup::Gsttt, "another").unwrap(),
            "no index of that repository is kept"
        );
        assert_eq!(read(&bed.gsttt(), "snapshot.gsttt"), "changed\n");
        assert_eq!(read(&bed.cache(), "project.db"), "changed\n");

        assert!(bed.indexes.restore(Setup::Gsttt, "repo").unwrap());
        assert_eq!(read(&bed.gsttt(), "snapshot.gsttt"), "snapshot\n");
        assert_eq!(
            read(&bed.cache(), "project.db"),
            "changed\n",
            "the cache is for the cbm setup"
        );
        assert!(bed.indexes.restore(Setup::Cbm, "repo").unwrap());
        assert_eq!(read(&bed.cache(), "project.db"), "graph\n");
    }

    #[test]
    fn a_missing_index_cannot_be_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut indexes = Indexes::default();
        let error = indexes
            .keep_cbm(&dir.path().join("nowhere"), &dir.path().join("aside"))
            .unwrap_err();
        assert!(error.to_string().contains("no index to keep"), "{error}");
    }
}
