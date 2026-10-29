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

/// Removes the directory of the session `id`, which is `<root>/<project>/<id>` for the project it ran in, and nothing else in `root`: the sessions of the user's other work are there too. The id is read from the stream of a run, so one that is not a UUID, the form Claude Code gives a session, removes nothing and is an error, and a link is never followed.
pub(super) fn remove(root: &Path, id: &str) -> anyhow::Result<()> {
    ensure!(
        is_uuid(id),
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

/// Whether `id` is a UUID: five groups of hexadecimal digits, of 8, 4, 4, 4 and 12, joined by `-`, in either case since RFC 4122 reads both. Nothing else can name a directory in a project: no `.`, no `/`, no empty id.
fn is_uuid(id: &str) -> bool {
    let mut groups = id.split('-');
    let shaped = [8, 4, 4, 4, 12].into_iter().all(|length| {
        groups.next().is_some_and(|group| {
            group.len() == length && group.bytes().all(|b| b.is_ascii_hexdigit())
        })
    });
    shaped && groups.next().is_none()
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
        let project = project_of(checkout);
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

    /// Removes what Claude Code made for the session `id`, whatever became of the run: `<project>/<id>` in the temporary root and in `projects` of the configuration directory, and `<id>` in every other directory of the configuration directory, which is `session-env/<id>`, where the client keeps the environment of a session. Each place is tried, and the first error is returned; nothing else goes, the sessions of the user's other work are in the same places.
    pub(super) fn remove_session(&self, id: &str) -> anyhow::Result<()> {
        let projects = self.config.join("projects");
        [&self.sessions, &self.config, &projects]
            .map(|root| remove(root, id))
            .into_iter()
            .collect()
    }

    /// Removes the project directory of every one of `checkouts` from the temporary root and from `projects` of the configuration directory when it is empty, and leaves it, without an error, when anything is in it or it is not there: the sessions of the user's other work in the same checkout are kept.
    pub(super) fn remove_projects<'a>(
        &self,
        checkouts: impl IntoIterator<Item = &'a Path>,
    ) -> anyhow::Result<()> {
        let projects = self.config.join("projects");
        for checkout in checkouts {
            let name = project_of(checkout);
            for root in [&self.sessions, &projects] {
                let dir = root.join(&name);
                match fs::remove_dir(&dir) {
                    Err(error)
                        if !matches!(
                            error.kind(),
                            io::ErrorKind::NotFound
                                | io::ErrorKind::DirectoryNotEmpty
                                | io::ErrorKind::NotADirectory
                        ) =>
                    {
                        return Err(error)
                            .with_context(|| format!("cannot remove {}", dir.display()));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

/// The name of the project directory of a run in `checkout`: [`project_name`] of its canonical path. A checkout that is not there yet has no canonical name, and no session has been made in it.
fn project_of(checkout: &Path) -> String {
    project_name(&fs::canonicalize(checkout).unwrap_or_else(|_| checkout.to_path_buf()))
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

    /// A session id as Claude Code makes it, and another.
    const ID: &str = "7d1f0f0e-3c2b-4a55-8c1e-2f6a9b0c4d11";
    const OTHER: &str = "7d1f0f0e-3c2b-4a55-8c1e-2f6a9b0c4d12";

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
            session(root.path(), "-a", ID),
            session(root.path(), "-b", ID),
        ];
        // A directory that only ends in the id is not the session's.
        let longer = format!("x{ID}");
        let others = [
            session(root.path(), "-a", OTHER),
            session(root.path(), "-c", &longer),
        ];
        fs::write(root.path().join("stray"), "").unwrap();

        remove(root.path(), ID).unwrap();

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
        let kept = session(root.path(), "-a", OTHER);
        remove(root.path(), ID).unwrap();
        assert!(kept.join("note").exists());
        remove(&root.path().join("not-there"), ID).unwrap();
    }

    #[gsttt_scenario::scenario(
        "agent-benchmarks",
        "Only a UUID names the session directory to remove"
    )]
    #[test]
    fn an_id_that_is_not_a_uuid_removes_nothing() {
        let root = tempfile::tempdir().unwrap();
        let kept = [
            session(root.path(), "-a", "x"),
            session(root.path(), "-a", ID),
        ];
        // Shapes close to a UUID, and the ones that would leave the project directory.
        let with_slash = format!("{ID}/");
        let with_newline = format!("{ID}\n");
        let with_group = format!("{ID}-0");
        let short: String = ID.chars().skip(1).collect();
        let no_dashes = ID.replace('-', "");
        let not_hex = ID.replacen('7', "g", 1);
        for id in [
            "",
            ".",
            "..",
            "../x",
            "-a/x",
            "a b",
            "a\n",
            "abc-123",
            &with_slash,
            &with_newline,
            &with_group,
            &short,
            &no_dashes,
            &not_hex,
        ] {
            let error = remove(root.path(), id).unwrap_err();
            assert!(error.to_string().contains("not a plain id"), "{id:?}");
            for dir in &kept {
                assert!(
                    dir.join("note").exists(),
                    "{id:?} removed {}",
                    dir.display()
                );
            }
        }
    }

    #[test]
    fn an_id_in_capitals_is_a_uuid_too() {
        let root = tempfile::tempdir().unwrap();
        let capitals = ID.to_uppercase();
        let session = session(root.path(), "-a", &capitals);
        remove(root.path(), &capitals).unwrap();
        assert!(!session.exists());
    }

    #[test]
    fn a_link_is_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        // A project that is a link to a directory with a session of that id in it, and a session that is a link.
        let behind_a_link = session(elsewhere.path(), "project", ID);
        symlink(elsewhere.path().join("project"), root.path().join("-link")).unwrap();
        let target = elsewhere.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("note"), "").unwrap();
        fs::create_dir(root.path().join("-b")).unwrap();
        symlink(&target, root.path().join("-b").join(ID)).unwrap();

        remove(root.path(), ID).unwrap();

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

    #[test]
    fn the_session_is_removed_from_the_temporary_root_and_the_configuration_directory() {
        let machine = machine();
        let (config, sessions) = (&machine.state.config, &machine.state.sessions);
        let projects = config.join("projects");
        // What the client makes for a session: a directory in a project of the temporary root and of `projects`, and `session-env/<id>` beside `projects`.
        let mine = [
            session(sessions, &machine.project, ID),
            session(&projects, &machine.project, ID),
            session(config, "session-env", ID),
        ];
        let others = [
            session(sessions, &machine.project, OTHER),
            session(&projects, &machine.project, OTHER),
            session(&projects, "-other", OTHER),
            session(config, "session-env", OTHER),
        ];
        put(&config.join("settings.json"));

        machine.state.remove_session(ID).unwrap();

        for gone in &mine {
            assert!(!gone.exists(), "{}", gone.display());
        }
        for kept in &others {
            assert!(kept.join("note").exists(), "{}", kept.display());
        }
        assert!(config.join("settings.json").exists());
        // An id that is not a UUID removes nothing anywhere and is an error.
        assert!(machine.state.remove_session("..").is_err());
        for kept in &others {
            assert!(kept.join("note").exists(), "{}", kept.display());
        }
    }

    #[test]
    fn a_project_directory_goes_when_it_is_empty_and_stays_when_it_is_not() {
        let machine = machine();
        let (config, sessions) = (&machine.state.config, &machine.state.sessions);
        let projects = config.join("projects");
        let nowhere = machine.base.join("runs/nowhere");
        // Empty in the temporary root, and another session in it in `projects`.
        fs::create_dir_all(sessions.join(&machine.project)).unwrap();
        let busy = session(&projects, &machine.project, OTHER);
        let other = session(&projects, "-other", OTHER);

        machine
            .state
            .remove_projects([machine.checkout.as_path(), nowhere.as_path()])
            .unwrap();

        assert!(!sessions.join(&machine.project).exists());
        assert!(busy.join("note").exists());
        assert!(other.join("note").exists());
        assert!(sessions.exists(), "the temporary root itself stays");
    }
}
