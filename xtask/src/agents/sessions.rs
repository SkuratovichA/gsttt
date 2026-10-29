//! What Claude Code keeps on this machine, and which of it a run may read: the directory it makes for a session in the temporary root that every session of the user shares, the directory of its configuration, and the files beside that in `$HOME`.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, ensure};

/// The temporary root of the Claude Code sessions of this user: `/tmp/claude-<uid>`, which is `/private/tmp/claude-<uid>` on macOS. It holds a directory per project, named after the working directory of its sessions, and a directory per session below it.
pub(super) fn root() -> PathBuf {
    PathBuf::from(format!(
        "/tmp/claude-{}",
        rustix::process::getuid().as_raw()
    ))
}

/// Removes the directory of the session `id`, which is `<root>/<project>/<id>` for the project it ran in, and nothing else in `root`: the sessions of the user's other work are there too. The id is read from the stream of a run, so one that is not a plain id removes nothing and is an error, and a link is never followed.
pub(super) fn remove(root: &Path, id: &str) -> anyhow::Result<()> {
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "the session id `{id}` is not a plain id, so no directory is removed for it"
    );
    let projects = match fs::read_dir(root) {
        Ok(projects) => projects,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", root.display())),
    };
    for project in projects {
        let project = project.with_context(|| format!("cannot read {}", root.display()))?;
        let session = project.path().join(id);
        let is_project = project.file_type().is_ok_and(|kind| kind.is_dir());
        if is_project && fs::symlink_metadata(&session).is_ok_and(|meta| meta.is_dir()) {
            fs::remove_dir_all(&session)
                .with_context(|| format!("cannot remove {}", session.display()))?;
        }
    }
    Ok(())
}

/// Where Claude Code keeps its state: the transcripts and the memory of the user, the logins, and the sessions of the user's other work. A run is denied all of it but the project directory of its own checkout, which holds what Claude Code saved of the run's own output when it was too big to show.
#[derive(Debug)]
pub(super) struct State {
    /// `$HOME`, which holds `.claude.json` and the configuration directory of every account of the user. None when the task has no `HOME`.
    pub(super) home: Option<PathBuf>,
    /// The configuration directory of the account of this task.
    pub(super) config: PathBuf,
    /// The temporary root that every session of the user shares: [`root`].
    pub(super) sessions: PathBuf,
}

impl State {
    /// The paths a run in `checkout` is denied, as the directories hold them now, sorted within each directory.
    ///
    /// Denied are every entry of `home` whose name starts with `.claude` but the configuration directory; every entry of the configuration directory but `projects`; and, of `projects` and of the temporary root, every entry but the project directory of `checkout`, and every entry that directory holds. The run's own session directory is made after this, so it is in no list. A path inside an entry that is denied is denied with it, so a link may deny a run the output it saved, and never opens anything.
    pub(super) fn denied(&self, checkout: &Path) -> anyhow::Result<Vec<PathBuf>> {
        // A checkout that is not there yet has no canonical name, and no session has been made in it.
        let canonical = fs::canonicalize(checkout).unwrap_or_else(|_| checkout.to_path_buf());
        let project = project_name(&canonical);
        let mut denied = Vec::new();
        if let Some(home) = &self.home {
            for entry in entries(home)? {
                let claude = entry
                    .file_name()
                    .is_some_and(|name| name.as_bytes().starts_with(b".claude"));
                if claude && !same_directory(&entry, &self.config) {
                    denied.push(entry);
                }
            }
        }
        for entry in entries(&self.config)? {
            if entry.file_name() != Some(OsStr::new("projects")) {
                denied.push(entry);
            }
        }
        denied.extend(all_but(&self.config.join("projects"), &project)?);
        denied.extend(all_but(&self.sessions, &project)?);
        Ok(denied)
    }
}

/// The name of the directory Claude Code makes for the project of a session that works in `canonical`, both in `projects` of its configuration directory and in the temporary root: the path with every character that is not a letter or a digit made `-`.
pub(super) fn project_name(canonical: &Path) -> String {
    canonical
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The entries of `root` but the project directory called `project`, and the entries that directory holds: a run reads what it makes there, and nothing that was there before it.
fn all_but(root: &Path, project: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut denied = Vec::new();
    for entry in entries(root)? {
        if entry.file_name() == Some(OsStr::new(project)) {
            denied.extend(entries(&entry)?);
        } else {
            denied.push(entry);
        }
    }
    Ok(denied)
}

/// The paths of the entries of `dir`, sorted; none when it is not there.
fn entries(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let listing = match fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", dir.display())),
    };
    let mut entries = Vec::new();
    for entry in listing {
        entries.push(
            entry
                .with_context(|| format!("cannot read {}", dir.display()))?
                .path(),
        );
    }
    entries.sort();
    Ok(entries)
}

/// Whether `first` and `second` are one directory, by whatever names they have.
fn same_directory(first: &Path, second: &Path) -> bool {
    match (fs::canonicalize(first), fs::canonicalize(second)) {
        (Ok(first), Ok(second)) => first == second,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::os::unix::fs::symlink;

    use super::*;

    /// A session directory with a file in it.
    fn session(root: &Path, project: &str, id: &str) -> PathBuf {
        let dir = root.join(project).join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("note"), "").unwrap();
        dir
    }

    #[test]
    fn the_directories_of_the_session_are_removed_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let mine = [
            session(root.path(), "-a", "abc-123"),
            session(root.path(), "-b", "abc-123"),
        ];
        let others = [
            session(root.path(), "-a", "abc-124"),
            session(root.path(), "-c", "xabc-123"),
        ];
        fs::write(root.path().join("stray"), "").unwrap();

        remove(root.path(), "abc-123").unwrap();

        for gone in &mine {
            assert!(!gone.exists(), "{}", gone.display());
        }
        for kept in &others {
            assert!(kept.join("note").exists(), "{}", kept.display());
        }
        assert!(root.path().join("stray").exists());
    }

    #[test]
    fn a_session_without_a_directory_and_a_root_that_is_not_there_are_fine() {
        let root = tempfile::tempdir().unwrap();
        let kept = session(root.path(), "-a", "other");
        remove(root.path(), "abc-123").unwrap();
        assert!(kept.join("note").exists());
        remove(&root.path().join("not-there"), "abc-123").unwrap();
    }

    #[test]
    fn an_id_that_is_not_a_plain_id_removes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let kept = session(root.path(), "-a", "x");
        for id in ["", ".", "..", "../x", "-a/x", "a b", "a\n"] {
            let error = remove(root.path(), id).unwrap_err();
            assert!(error.to_string().contains("not a plain id"), "{id:?}");
            assert!(kept.join("note").exists(), "{id:?}");
        }
    }

    #[test]
    fn a_link_is_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        // A project that is a link to a directory with a session of that id in it, and a session that is a link.
        let behind_a_link = session(elsewhere.path(), "project", "abc-123");
        symlink(elsewhere.path().join("project"), root.path().join("-link")).unwrap();
        let target = elsewhere.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("note"), "").unwrap();
        fs::create_dir(root.path().join("-b")).unwrap();
        symlink(&target, root.path().join("-b/abc-123")).unwrap();

        remove(root.path(), "abc-123").unwrap();

        assert!(behind_a_link.join("note").exists());
        assert!(target.join("note").exists());
    }

    /// A file at `path`, and the directories above it.
    fn put(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    /// What a machine of the user holds: the account in `home/.claude`, another beside it, `.claude.json` and what is not Claude Code's, the temporary root, and a checkout whose project directory is in the configuration directory and in the root. Everything is under the canonical name of a temporary directory.
    struct Machine {
        _dir: tempfile::TempDir,
        base: PathBuf,
        state: State,
        checkout: PathBuf,
        project: String,
    }

    fn machine() -> Machine {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let checkout = base.join("runs/vite");
        fs::create_dir_all(&checkout).unwrap();
        let state = State {
            home: Some(base.join("home")),
            config: base.join("home/.claude"),
            sessions: base.join("sessions"),
        };
        Machine {
            project: project_name(&checkout),
            _dir: dir,
            base,
            state,
            checkout,
        }
    }

    impl Machine {
        fn home(&self) -> &Path {
            self.state.home.as_deref().unwrap()
        }

        /// The project directory of the checkout in `root`, and a file in it.
        fn own(&self, root: &Path, file: &str) -> PathBuf {
            root.join(&self.project).join(file)
        }

        fn denied(&self) -> BTreeSet<PathBuf> {
            self.state
                .denied(&self.checkout)
                .unwrap()
                .into_iter()
                .collect()
        }
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "The project directory is named after the canonical path of its checkout"
    )]
    #[test]
    fn the_project_directory_is_named_after_the_path_of_the_checkout() {
        // As Claude Code named it for a run in the checkout of vite on 29.10.
        assert_eq!(
            project_name(Path::new(
                "/private/var/folders/8b/t787z9xj78lggd7wng56cxsh0000gn/T/gsttt-agents-runs/vite"
            )),
            "-private-var-folders-8b-t787z9xj78lggd7wng56cxsh0000gn-T-gsttt-agents-runs-vite"
        );
        // Anything but a letter or a digit becomes a dash: a dot, an underscore, a space, a letter with an accent.
        assert_eq!(project_name(Path::new("/a.b_c d/\u{e9}1")), "-a-b-c-d--1");
        // The name is the one of the canonical path: a link to the checkout is named after what it leads to.
        let machine = machine();
        let link = machine.base.join("link-to-vite");
        symlink(&machine.checkout, &link).unwrap();
        let old = machine.own(&machine.state.sessions, "old");
        put(&old);
        assert_eq!(machine.state.denied(&link).unwrap(), [old]);
    }

    #[test]
    fn every_entry_of_the_state_is_denied_but_the_project_directory_of_the_checkout() {
        let machine = machine();
        let (home, config, sessions) = (
            machine.home(),
            &machine.state.config,
            &machine.state.sessions,
        );
        // Claude Code's: the file beside the directories, the account of the task, another account, and the temporary root.
        for file in [
            ".claude.json",
            ".claude.json.backup",
            ".claude-other/CLAUDE.md",
        ] {
            put(&home.join(file));
        }
        for file in [
            "settings.json",
            "history.jsonl",
            "projects/-other/old.jsonl",
        ] {
            put(&config.join(file));
        }
        put(&machine.own(&config.join("projects"), "old.jsonl"));
        put(&machine.own(&config.join("projects"), "old-session/tool-results/out.txt"));
        put(&sessions.join("-other/session/note"));
        put(&sessions.join("stray"));
        put(&machine.own(sessions, "old-session/note"));
        // Not Claude Code's: other files of the home directory, and a name that only has `.claude` inside.
        for file in [".bashrc", "notes/.claude.json", "claude-notes"] {
            put(&home.join(file));
        }

        let expected: BTreeSet<PathBuf> = [
            home.join(".claude.json"),
            home.join(".claude.json.backup"),
            home.join(".claude-other"),
            config.join("settings.json"),
            config.join("history.jsonl"),
            config.join("projects/-other"),
            machine.own(&config.join("projects"), "old.jsonl"),
            machine.own(&config.join("projects"), "old-session"),
            sessions.join("-other"),
            sessions.join("stray"),
            machine.own(sessions, "old-session"),
        ]
        .into_iter()
        .collect();
        assert_eq!(machine.denied(), expected);
        // The way down to the saved output is open: the account, its projects and the project directory of the checkout are in no list, and nor is anything made after it.
        for open in [
            home.to_path_buf(),
            config.clone(),
            config.join("projects"),
            machine.own(&config.join("projects"), ""),
            sessions.clone(),
            machine.own(sessions, ""),
        ] {
            assert!(
                !machine.denied().contains(open.as_path()),
                "{}",
                open.display()
            );
        }

        // The directories are listed at every call, and a run has its own session directory made after it.
        put(&home.join(".claude-made-later"));
        put(&config.join("made-later"));
        put(&machine.own(&config.join("projects"), "made-later"));
        put(&sessions.join("-made-later/note"));
        let later = machine.denied();
        for made in [
            home.join(".claude-made-later"),
            config.join("made-later"),
            machine.own(&config.join("projects"), "made-later"),
            sessions.join("-made-later"),
        ] {
            assert!(later.contains(&made), "{}", made.display());
        }
        assert_eq!(later.len(), expected.len() + 4);
    }

    #[test]
    fn a_configuration_directory_outside_home_is_gone_down_all_the_same() {
        let mut machine = machine();
        let account = machine.base.join("account");
        machine.state.config = account.clone();
        let home = machine.home().to_path_buf();
        put(&home.join(".claude.json"));
        put(&home.join(".claude/CLAUDE.md"));
        put(&account.join("CLAUDE.md"));
        put(&account.join("projects/-other/old.jsonl"));
        put(&machine.own(&account.join("projects"), "old.jsonl"));

        // No directory of the home is the account, so every `.claude*` of it is denied whole.
        assert_eq!(
            machine.denied(),
            BTreeSet::from([
                home.join(".claude.json"),
                home.join(".claude"),
                account.join("CLAUDE.md"),
                account.join("projects/-other"),
                machine.own(&account.join("projects"), "old.jsonl"),
            ])
        );
    }

    #[test]
    fn a_link_may_deny_a_run_its_saved_output_and_opens_nothing() {
        // The layout of the machine of the user: the account in `.claude-b` shares the projects of `.claude` by a link.
        let mut machine = machine();
        let home = machine.home().to_path_buf();
        let shared = home.join(".claude/projects");
        put(&shared.join("-other/old.jsonl"));
        put(&machine.own(&shared, "old.jsonl"));
        let account = home.join(".claude-b");
        fs::create_dir_all(&account).unwrap();
        symlink(&shared, account.join("projects")).unwrap();
        put(&account.join("settings.json"));
        machine.state.config = account.clone();

        let denied = machine.denied();
        // The other account is denied whole, which includes the projects behind the link: the run cannot read its saved output.
        assert!(denied.contains(&home.join(".claude")));
        // Through the link, the entries are denied by the name they have there, and the project directory of the checkout is not.
        assert!(denied.contains(&account.join("projects/-other")));
        assert!(denied.contains(&machine.own(&account.join("projects"), "old.jsonl")));
        assert!(!denied.contains(&machine.own(&account.join("projects"), "")));
        assert!(denied.contains(&account.join("settings.json")));
        assert!(!denied.contains(&account));
    }

    #[test]
    fn nothing_is_denied_where_nothing_is_kept() {
        let machine = machine();
        // No directory of the state is there, and the home directory holds nothing of Claude Code's.
        assert_eq!(machine.denied(), BTreeSet::new());
        put(&machine.home().join(".bashrc"));
        assert_eq!(machine.denied(), BTreeSet::new());
        let none = State {
            home: None,
            config: machine.base.join("nowhere"),
            sessions: machine.base.join("nowhere-else"),
        };
        assert_eq!(
            none.denied(&machine.checkout).unwrap(),
            Vec::<PathBuf>::new()
        );
    }
}
