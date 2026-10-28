//! A source over a fixed index that counts what it is asked and can be told to fail.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use gsttt_core::engine::changes::{Change, ChangeKind};
use gsttt_core::path::PathError;
use gsttt_core::{Freshness, Index, RepoPath};
use gsttt_mcp::{IndexSource, Loaded, SourceError, Target};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::fixture::Tree;

/// How many times a slow refresh gives up its turn on the processor while it waits for another call to arrive. It is
/// the most a passing test waits, and it is long enough for calls sent together to be inside the source together.
const LINGERS: usize = 50_000;

/// A source the tests keep a handle to, so that they can read what the server asked after it has run. Clones share
/// one set of counters.
#[derive(Clone, Debug)]
pub(crate) struct Fake(Arc<Inner>);

#[derive(Debug)]
struct Inner {
    /// What a refresh returns.
    loaded: Loaded,
    /// What a read of the stored index returns: `None` before the first index.
    stored: Option<Loaded>,
    /// Paths with no file on disk.
    missing: Vec<String>,
    /// The files that differ from `HEAD`, whatever the base.
    changes: Vec<Change>,
    /// The failure every call ends in, when the source is down.
    failure: Option<String>,
    /// Whether a refresh stays in progress until another call arrives, so that calls that overlap are seen to.
    slow_refresh: bool,
    /// Whether the next refresh panics. The one after it does not.
    breaks_once: AtomicBool,
    /// Where the first refresh waits until the test lets it go.
    held: Option<Held>,

    /// Calls of any kind, failed ones included.
    calls: AtomicUsize,
    refreshes: AtomicUsize,
    stored_reads: AtomicUsize,
    paths: Mutex<Vec<String>>,
    bases: Mutex<Vec<Option<String>>>,
    /// Calls in progress now, and the most there ever were.
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Fake {
    /// A source over the smallest tree there is to ask about: `src/a.ts` imports `lib/util.ts`.
    pub(crate) fn small() -> Self {
        Self::over(Tree::new().imports("src/a.ts", "lib/util.ts").build())
    }

    /// A source that holds `index`, fresh, as both its stored and its refreshed index.
    pub(crate) fn over(index: Index) -> Self {
        let loaded = Loaded {
            index: Arc::new(index),
            freshness: Freshness::Fresh,
        };
        Self(Arc::new(Inner {
            stored: Some(loaded.clone()),
            loaded,
            missing: Vec::new(),
            changes: Vec::new(),
            failure: None,
            slow_refresh: false,
            breaks_once: AtomicBool::new(false),
            held: None,
            calls: AtomicUsize::new(0),
            refreshes: AtomicUsize::new(0),
            stored_reads: AtomicUsize::new(0),
            paths: Mutex::new(Vec::new()),
            bases: Mutex::new(Vec::new()),
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }))
    }

    /// A source that fails every call with `message`.
    pub(crate) fn failing(mut self, message: &str) -> Self {
        self.inner().failure = Some(message.to_owned());
        self
    }

    /// A source that has not built an index yet: it has none stored.
    pub(crate) fn without_stored_index(mut self) -> Self {
        self.inner().stored = None;
        self
    }

    /// A source whose stored index is behind a tree that has moved on.
    pub(crate) fn stored_stale(mut self, added: u32, removed: u32, changed: u32) -> Self {
        if let Some(stored) = &mut self.inner().stored {
            stored.freshness = Freshness::Stale {
                added,
                removed,
                changed,
            };
        }
        self
    }

    /// A source in which `path` has no file on disk.
    pub(crate) fn not_on_disk(mut self, path: &str) -> Self {
        self.inner().missing.push(path.to_owned());
        self
    }

    /// A source whose tree differs from `HEAD` by `changes`, whatever the base.
    pub(crate) fn changes(mut self, changes: &[(&str, ChangeKind)]) -> Self {
        self.inner().changes = changes
            .iter()
            .map(|&(path, kind)| Change {
                path: RepoPath::normalize(path).expect("a path from the root"),
                kind,
            })
            .collect();
        self
    }

    /// A source whose refresh stays in progress until another call arrives, or until it is clear that none will.
    /// With one call at a time it never sees another, so it only costs a moment; it is how a test gives a missing
    /// lock time to show.
    pub(crate) fn slow_refresh(mut self) -> Self {
        self.inner().slow_refresh = true;
        self
    }

    /// A source whose first refresh panics, as code behind a source may. Later refreshes answer.
    pub(crate) fn breaking_once(self) -> Self {
        self.0.breaks_once.store(true, Ordering::SeqCst);
        self
    }

    /// A source whose first refresh stays in progress until the returned [`Hold`] lets it go.
    pub(crate) fn holding_first_refresh(mut self) -> (Self, Hold) {
        let (entered_tx, entered) = unbounded_channel();
        let (release, release_rx) = mpsc::channel();
        self.inner().held = Some(Held {
            entered: entered_tx,
            release: Mutex::new(release_rx),
            used: AtomicBool::new(false),
        });
        (self, Hold { entered, release })
    }

    fn inner(&mut self) -> &mut Inner {
        Arc::get_mut(&mut self.0).expect("a source is configured before it is shared")
    }

    /// Every call made, failed or not.
    pub(crate) fn calls(&self) -> usize {
        self.0.calls.load(Ordering::SeqCst)
    }

    /// Calls of `refreshed`.
    pub(crate) fn refreshes(&self) -> usize {
        self.0.refreshes.load(Ordering::SeqCst)
    }

    /// Calls of `stored`.
    pub(crate) fn stored_reads(&self) -> usize {
        self.0.stored_reads.load(Ordering::SeqCst)
    }

    /// The path arguments `path` was asked about, in order.
    pub(crate) fn paths_read(&self) -> Vec<String> {
        self.0
            .paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The bases `changed` was asked to compare against, in order.
    pub(crate) fn bases_read(&self) -> Vec<Option<String>> {
        self.0
            .bases
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The index that a refresh returns, to work out what the command line prints for it.
    pub(crate) fn index(&self) -> Arc<Index> {
        Arc::clone(&self.0.loaded.index)
    }

    /// The index that a read of the stored index returns, with how it stands against the tree.
    pub(crate) fn stored_index(&self) -> Option<Loaded> {
        self.0.stored.clone()
    }

    /// The most calls that were ever in progress at once.
    pub(crate) fn peak(&self) -> usize {
        self.0.peak.load(Ordering::SeqCst)
    }
}

/// The source's end of a refresh that is held.
#[derive(Debug)]
struct Held {
    entered: UnboundedSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    /// Whether a refresh has been held already: only the first is.
    used: AtomicBool,
}

impl Held {
    /// Tells the test that a refresh is in progress, and waits there until the test lets it go.
    fn wait(&self) {
        if self.used.swap(true, Ordering::SeqCst) {
            return;
        }
        self.entered.send(()).ok();
        let release = self.release.lock().unwrap_or_else(PoisonError::into_inner);
        release.recv().ok();
    }
}

/// The test's end of a refresh that is held.
#[derive(Debug)]
pub(crate) struct Hold {
    entered: UnboundedReceiver<()>,
    release: mpsc::Sender<()>,
}

impl Hold {
    /// Waits until the refresh is in progress and held.
    pub(crate) async fn entered(&mut self) {
        self.entered.recv().await.expect("a refresh was started");
    }

    /// Lets the held refresh go on.
    pub(crate) fn release(&self) {
        self.release
            .send(())
            .expect("the held refresh is waiting to be released");
    }
}

/// One call in progress. Dropping it ends the call.
struct Busy<'a>(&'a Inner);

impl Inner {
    /// Counts a call and notes how many are in progress with it.
    fn begin(&self) -> Result<Busy<'_>, SourceError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        let busy = Busy(self);
        match &self.failure {
            Some(message) => Err(SourceError::new(message.as_str())),
            None => Ok(busy),
        }
    }

    /// Stays inside a call until a second call is inside too, or until the wait is over.
    fn linger(&self) {
        for _ in 0..LINGERS {
            if self.active.load(Ordering::SeqCst) > 1 {
                return;
            }
            std::thread::yield_now();
        }
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IndexSource for Fake {
    fn refreshed(&self) -> Result<Loaded, SourceError> {
        let _busy = self.0.begin()?;
        let breaks = self.0.breaks_once.swap(false, Ordering::SeqCst);
        assert!(
            !breaks,
            "the code behind the source broke while it refreshed"
        );
        if let Some(held) = &self.0.held {
            held.wait();
        }
        self.0.refreshes.fetch_add(1, Ordering::SeqCst);
        if self.0.slow_refresh {
            self.0.linger();
        }
        Ok(self.0.loaded.clone())
    }

    fn stored(&self) -> Result<Option<Loaded>, SourceError> {
        let _busy = self.0.begin()?;
        self.0.stored_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.stored.clone())
    }

    fn path(&self, argument: &str) -> Result<Target, SourceError> {
        let _busy = self.0.begin()?;
        self.0
            .paths
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(argument.to_owned());
        let path = RepoPath::normalize(argument).map_err(|error| match error {
            PathError::EscapesRoot(raw) => SourceError::new(format!(
                "`{raw}` is outside the repository; give a path relative to the repository root"
            )),
            other => SourceError::new(other.to_string()),
        })?;
        let on_disk = !self
            .0
            .missing
            .iter()
            .any(|missing| missing == path.as_str());
        Ok(Target { path, on_disk })
    }

    fn changed(&self, base: Option<&str>) -> Result<Vec<Change>, SourceError> {
        let _busy = self.0.begin()?;
        self.0
            .bases
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(base.map(str::to_owned));
        Ok(self.0.changes.clone())
    }
}
