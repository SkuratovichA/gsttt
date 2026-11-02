//! The cross-file half of routing. `parse` records, for every file, the routes declared on routers, the prefix each router declares for itself and the routers the file mounts under a prefix; here the mounts are followed from the router of each route up to the routers nobody mounts, and the route is served once for every way up, behind the prefixes of every mount and router on the way.
//!
//! A router's routes are no facts of its file, so the path a router serves on its own appears only when no mount reaches it. A mount names its router by an expression of its own file, and a package that re-exports the router under another name is followed to the module that makes it.
//!
//! A way that cannot be followed to the end is no path, and the line it stopped at is an unread site of its file: a mount or a router whose prefix is not text, or a way that meets a bound of the walk.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{LinkFile, LinkInput, Linked, LinkedFact};
use gsttt_core::contract::ContractKind;
use gsttt_core::fact::{ContractFact, Line, UnresolvedReason, UnresolvedSite};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::rules::{Reading, contract_key};

use crate::extra::{Mount, PyExtra, Target};
use crate::project::PythonProject;
use crate::resolve::module_file;
use crate::site::join_prefixes;

/// How many mounts are followed upward from one router, so that a very deep tree of mounts cannot exhaust the stack. Routers that mount each other end sooner, at the first router seen twice.
pub(crate) const MAX_DEPTH: usize = 32;

/// How many ways up are kept for one router: the first by path. Mounts shared at every level multiply the ways, and a router mounted in dozens of places is unlike any real application.
pub(crate) const MAX_WAYS: usize = 64;

/// How many ways up the walk from one router ends, whole or cut, before it stops. The ways kept are the first by path among these, so that moving a mount in a file changes them only when it adds or removes a way.
const MAX_WALKED: usize = 16 * MAX_WAYS;

/// How many imports are followed from the name a mount writes to the module that makes the router. A re-export is one or two imports deep, and modules that import each other's names end at the bound.
const MAX_HOPS: usize = 8;

/// A router: the file it lives in and the variable that holds it. Django's `urlpatterns` is the module itself and has the empty name.
type Router<'a> = (FileId, &'a str);

/// Finds the file of the module a specifier names, as the file `from` imports it.
type ModuleFile<'f> = &'f dyn Fn(&RepoPath, &str) -> Option<FileId>;

/// What the names of the files of a project stand for: the files themselves and where their imports lead.
struct Names<'i, 'a> {
    input: &'i LinkInput<'a, PyExtra>,
    module_file: ModuleFile<'i>,
}

/// A mount seen from the router it mounts. The order is by content, so that the walk goes through the mounts of a router in an order the files do not decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Parent<'a> {
    /// The router the mount is called on: `app` in `app.include_router(…)`.
    router: Router<'a>,
    /// The prefix the mount gives; [`Reading::Unread`] when it is not text.
    prefix: &'a Reading,
    replaces: bool,
    /// Where the mount is and the call as written, for a cut way to name.
    file: FileId,
    line: Line,
    callee: &'a str,
}

/// Why a way up from a router was not followed to its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Why {
    /// The way came back to a router it had passed: routers mounted into each other.
    Cycle,
    TooDeep,
    /// The walk reached its bound, or the way was one of those beyond the ones kept.
    TooMany,
    /// The mount gives a prefix that is not text, so the path behind it is not known. `parse` has reported the mount, so `link` does not.
    MountPrefix,
    /// The router gives its own prefix that is not text, and no registration with a prefix of its own stands in for it, so the path behind the router is not known.
    OwnPrefix,
}

/// A line that a way up from a router stopped at, which coverage counts as a site `link` could not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cut<'a> {
    file: FileId,
    line: Line,
    /// The code at `line` as written: the mount call, or the decorator of a route.
    detail: &'a str,
    why: Why,
}

impl Cut<'_> {
    /// The site coverage counts for this cut, `None` for one `parse` has counted already.
    fn unread(&self) -> Option<(FileId, UnresolvedSite)> {
        let reason = match self.why {
            Why::MountPrefix => return None,
            Why::OwnPrefix => {
                return Some((
                    self.file,
                    UnresolvedSite::non_literal(self.line, self.detail),
                ));
            }
            Why::Cycle => "routers that include each other".to_owned(),
            Why::TooDeep => format!("routers mounted more than {MAX_DEPTH} levels deep"),
            Why::TooMany => format!("more than {MAX_WAYS} ways up from a router"),
        };
        Some((
            self.file,
            UnresolvedSite {
                line: self.line,
                reason: UnresolvedReason::Unsupported(reason),
                detail: self.detail.to_owned(),
            },
        ))
    }
}

/// What the router a route is declared on makes of it: the prefixes it is served behind, and whether its own prefix, which is not text, cut a way.
struct Ways {
    prefixes: Vec<String>,
    /// Each route of the router is then an unread site of its own, because the router that holds the unread prefix is the one the routes are declared on.
    own_unread: bool,
}

/// The mounts of one project, read from the `extra` of its files.
struct Mounts<'a> {
    /// The mounts by the router they mount.
    parents: BTreeMap<Router<'a>, Vec<Parent<'a>>>,
    /// The prefix each router declares for itself, when it declares one.
    own: BTreeMap<Router<'a>, &'a Reading>,
}

/// What `link` made, and the ways it could not follow.
struct Composed<'a> {
    facts: Vec<LinkedFact>,
    cuts: BTreeSet<Cut<'a>>,
}

/// The routes declared on routers, once for each way their router is mounted, behind the prefixes of the mounts and of the routers above it. Each line a way was cut at is an unread site of its file, except the mounts `parse` has reported.
pub(crate) fn link(project: &PythonProject, input: &LinkInput<'_, PyExtra>) -> Linked {
    let Composed { facts, cuts } = compose(input, &|from, spec| module_file(project, from, spec));
    Linked {
        facts,
        unread: cuts.iter().filter_map(Cut::unread).collect(),
    }
}

fn compose<'a>(input: &LinkInput<'a, PyExtra>, module_file: ModuleFile<'_>) -> Composed<'a> {
    let mounts = Mounts::read(&Names { input, module_file });
    let mut cuts = BTreeSet::new();
    // The ways up depend on the router alone, so every route of a router shares them.
    let mut ways: BTreeMap<Router<'a>, Ways> = BTreeMap::new();
    let mut facts = Vec::new();
    for file in input.files().iter().copied() {
        for route in &file.extra.routes {
            let Some((method, path)) = route.fact.key.http_parts() else {
                continue;
            };
            let router = (file.id, route.var.as_str());
            let ways = ways
                .entry(router)
                .or_insert_with(|| mounts.prefixes(router, &mut cuts));
            if ways.own_unread {
                cuts.insert(Cut {
                    file: file.id,
                    line: route.fact.line,
                    detail: &route.callee,
                    why: Why::OwnPrefix,
                });
            }
            for prefix in &ways.prefixes {
                facts.push(LinkedFact::found(
                    file.id,
                    ContractFact {
                        key: contract_key(&ContractKind::Http, Some(prefix), path, Some(method)),
                        role: route.fact.role,
                        line: route.fact.line,
                        via: route.fact.via.clone(),
                    },
                ));
            }
        }
    }
    // Two routes of one file can be reached the same way, and the engine only sorts by file.
    facts.sort_by(|a, b| {
        (a.file, a.fact.line, a.fact.key.key()).cmp(&(b.file, b.fact.line, b.fact.key.key()))
    });
    facts.dedup();
    Composed { facts, cuts }
}

impl<'a> Mounts<'a> {
    fn read(names: &Names<'_, 'a>) -> Self {
        let mut parents: BTreeMap<Router<'a>, Vec<Parent<'a>>> = BTreeMap::new();
        let mut own = BTreeMap::new();
        for file in names.input.files().iter().copied() {
            for mount in &file.extra.mounts {
                if let Some(child) = names.mounted(file, mount) {
                    // The receiver may be a router imported from another file, which then holds its prefix.
                    let router = names
                        .named(file, &mount.receiver, 0)
                        .unwrap_or((file.id, mount.receiver.as_str()));
                    parents.entry(child).or_default().push(Parent {
                        router,
                        prefix: &mount.prefix,
                        replaces: mount.replaces,
                        file: file.id,
                        line: mount.line,
                        callee: &mount.callee,
                    });
                }
            }
            for router in &file.extra.routers {
                own.insert((file.id, router.var.as_str()), &router.prefix);
            }
        }
        for mounts in parents.values_mut() {
            mounts.sort_unstable();
        }
        Self { parents, own }
    }

    /// The prefix `router` declares for itself, [`Reading::Absent`] when it declares none.
    fn own_prefix(&self, router: Router<'a>) -> &'a Reading {
        self.own.get(&router).copied().unwrap_or(&Reading::Absent)
    }

    /// Every prefix `start` is served behind, sorted: one for each way up through the mounts to a router nobody mounts, the first [`MAX_WAYS`] by path when there are more. A router that no mount reaches is served behind its own prefix. One whose every way up is cut is mounted, by a mount that cannot be followed to its end, so it is served behind no prefix that is known.
    fn prefixes(&self, start: Router<'a>, cuts: &mut BTreeSet<Cut<'a>>) -> Ways {
        let mut climb = Climb {
            mounts: self,
            ways: BTreeMap::new(),
            ended: 0,
            own_unread: false,
            cuts,
        };
        climb.up(start, "", &mut vec![start], None);
        if let Some(first_beyond) = climb.ways.keys().nth(MAX_WAYS).cloned() {
            for mount in climb.ways.split_off(&first_beyond).into_values().flatten() {
                climb.cuts.insert(mount);
            }
        }
        Ways {
            prefixes: climb.ways.into_keys().collect(),
            own_unread: climb.own_unread,
        }
    }
}

/// One climb from a router to the routers above it.
struct Climb<'m, 'a, 'c> {
    mounts: &'m Mounts<'a>,
    /// The prefix of each way that reached a router nobody mounts, with the last mount the way passed, which is the one to name when the way is not among those kept.
    ways: BTreeMap<String, Option<Cut<'a>>>,
    /// Ways ended so far, whole or cut.
    ended: usize,
    /// Whether a way was cut at the own prefix of the router the climb started from, which no mount stands below.
    own_unread: bool,
    cuts: &'c mut BTreeSet<Cut<'a>>,
}

impl<'a> Climb<'_, 'a, '_> {
    /// Climbs from `node`. `below` is the path between `node` and the route: everything the routers and mounts below `node` put in front of it, except `node`'s own prefix, which a replacing mount may drop. `trail` holds the routers passed, `node` last, and `via` the mount that led to `node`.
    fn up(
        &mut self,
        node: Router<'a>,
        below: &str,
        trail: &mut Vec<Router<'a>>,
        via: Option<Cut<'a>>,
    ) {
        let own = written(self.mounts.own_prefix(node));
        let Some(parents) = self.mounts.parents.get(&node) else {
            match own {
                Some(own) => {
                    self.ended += 1;
                    self.ways
                        .insert(join_prefixes(&ContractKind::Http, own, below), via);
                }
                None => self.cut_own(via),
            }
            return;
        };
        for parent in parents {
            let mount = Cut {
                file: parent.file,
                line: parent.line,
                detail: parent.callee,
                why: Why::TooMany,
            };
            let bound = if self.ended >= MAX_WALKED {
                Some(Why::TooMany)
            } else if trail.contains(&parent.router) {
                Some(Why::Cycle)
            } else if trail.len() >= MAX_DEPTH {
                Some(Why::TooDeep)
            } else {
                None
            };
            if let Some(why) = bound {
                self.cut_mount(why, mount);
                continue;
            }
            // FastAPI and Django put the mount's prefix in front of the router's own; Flask's registration prefix stands where the blueprint's own would have been, and the blueprint keeps its own only without one. A prefix that is not text cuts the way unless a registration prefix replaces it.
            let local = match (parent.prefix, parent.replaces, own) {
                (Reading::Unread, ..) => {
                    self.cut_mount(Why::MountPrefix, mount);
                    continue;
                }
                (Reading::Text(prefix), true, _) => Cow::Borrowed(prefix.as_str()),
                (_, _, None) => {
                    self.cut_own(via);
                    continue;
                }
                (Reading::Absent, true, Some(own)) => Cow::Borrowed(own),
                (prefix, false, Some(own)) => Cow::Owned(join_prefixes(
                    &ContractKind::Http,
                    written(prefix).unwrap_or_default(),
                    own,
                )),
            };
            let below = join_prefixes(&ContractKind::Http, &local, below);
            trail.push(parent.router);
            self.up(parent.router, &below, trail, Some(mount));
            trail.pop();
        }
    }

    /// Ends a way at `mount`, the mount to a router that is not followed.
    fn cut_mount(&mut self, why: Why, mount: Cut<'a>) {
        self.ended += 1;
        self.cuts.insert(Cut { why, ..mount });
    }

    /// Ends a way at the own prefix of the router the climb is at, which is not text. The unread site is the mount `via` that leads to the router from below; the router the climb started from has none, and its routes are the sites.
    fn cut_own(&mut self, via: Option<Cut<'a>>) {
        self.ended += 1;
        match via {
            Some(mount) => {
                self.cuts.insert(Cut {
                    why: Why::OwnPrefix,
                    ..mount
                });
            }
            None => self.own_unread = true,
        }
    }
}

/// The text of a prefix that is written, empty when there is none, and `None` when there is one that is not text.
fn written(reading: &Reading) -> Option<&str> {
    match reading {
        Reading::Text(text) => Some(text),
        Reading::Absent => Some(""),
        Reading::Unread => None,
    }
}

impl<'a> Names<'_, 'a> {
    /// The router `mount` of `file` mounts, when it can be found.
    fn mounted(&self, file: LinkFile<'a, PyExtra>, mount: &'a Mount) -> Option<Router<'a>> {
        match &mount.target {
            Target::Module(spec) => Some(((self.module_file)(file.path, spec)?, "")),
            Target::Name(name) => self.named(file, name, 0),
        }
    }

    /// The router an expression names in `file`: a variable of the file itself, or a name its imports bind. A module that holds the name only by importing it from another, as a package does that re-exports a router, is followed on to the module that makes it; `hops` counts the imports followed so far.
    fn named(&self, file: LinkFile<'a, PyExtra>, expr: &'a str, hops: usize) -> Option<Router<'a>> {
        if hops > MAX_HOPS {
            return None;
        }
        let (head, rest) = expr.split_once('.').unwrap_or((expr, ""));
        let Some(bind) = file
            .extra
            .binds
            .iter()
            .rev()
            .find(|bind| bind.local == head)
        else {
            return Some((file.id, expr));
        };
        let (module, name) = match (bind.name.as_deref(), rest) {
            // `import app.api.bookings as bookings`: the alias is the module, and a module is no router.
            (None, "") => return None,
            // `import app.api.bookings as bookings; bookings.router`: the router is inside the module.
            (None, rest) => ((self.module_file)(file.path, &bind.spec)?, rest),
            // `from app.api.bookings import router`: the module holds the router, even when it is also called `router`.
            (Some(name), "") => ((self.module_file)(file.path, &bind.spec)?, name),
            // `from app.api import bookings; bookings.router`: the name is a module of the package, and the router is inside it.
            (Some(name), rest) => {
                let spec = submodule_spec(&bind.spec, name);
                ((self.module_file)(file.path, &spec)?, rest)
            }
        };
        match self.input.get(module) {
            Some(holder) => self.named(*holder, name, hops + 1),
            // A file of another project is not in the input: its router is taken as named.
            None => Some((module, name)),
        }
    }
}

/// The specifier of the module `name` inside the package `spec`: `app.api` and `bookings` give `app.api.bookings`, and `.` gives `.bookings`.
fn submodule_spec(spec: &str, name: &str) -> String {
    if spec.ends_with('.') {
        format!("{spec}{name}")
    } else {
        format!("{spec}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use gsttt_core::contract::{ContractKey, Role};
    use std::fmt::Write;

    use gsttt_core::fact::FileFacts;

    use super::*;
    use crate::extra::{Bind, Route, RouterPrefix};

    /// The mounts of a project with one file, `app.py`, whose routers are all variables of that file.
    struct Project {
        path: RepoPath,
        facts: FileFacts,
        extra: PyExtra,
    }

    impl Project {
        fn new() -> Self {
            Self {
                path: RepoPath::normalize("app.py").unwrap(),
                facts: FileFacts::default(),
                extra: PyExtra::default(),
            }
        }

        /// `parent.include_router(child, prefix=prefix)` on the next line.
        fn mount(&mut self, parent: &str, child: &str, prefix: &str) {
            self.mount_reading(parent, child, Reading::Text(prefix.to_owned()), false);
        }

        /// `parent.register_blueprint(child, url_prefix=prefix)` on the next line, which gives `prefix` in place of the prefix of `child`.
        fn register(&mut self, parent: &str, child: &str, prefix: Reading) {
            self.mount_reading(parent, child, prefix, true);
        }

        /// A mount on the next line whose prefix is `prefix`.
        fn mount_reading(&mut self, parent: &str, child: &str, prefix: Reading, replaces: bool) {
            let line = Line::try_from(self.extra.mounts.len() + 1).unwrap();
            self.extra.mounts.push(Mount {
                line,
                receiver: parent.to_owned(),
                callee: format!("{parent}.include_router"),
                target: Target::Name(child.to_owned()),
                prefix,
                replaces,
            });
        }

        fn declare(&mut self, router: &str, prefix: &str) {
            self.extra.routers.push(RouterPrefix {
                var: router.to_owned(),
                prefix: Reading::Text(prefix.to_owned()),
            });
        }

        /// A router whose constructor gives a prefix that is not text.
        fn declare_unread(&mut self, router: &str) {
            self.extra.routers.push(RouterPrefix {
                var: router.to_owned(),
                prefix: Reading::Unread,
            });
        }

        fn route(&mut self, router: &str, path: &str) {
            self.extra.routes.push(Route {
                var: router.to_owned(),
                callee: format!("{router}.get"),
                fact: ContractFact {
                    key: ContractKey::http("GET", path),
                    role: Role::Serves,
                    line: 1,
                    via: "fastapi".to_owned(),
                },
            });
        }

        fn compose(&self) -> Composed<'_> {
            self.compose_in(&|_, _| None)
        }

        /// Like [`Project::compose`], with `module_file` finding the files that specifiers name.
        fn compose_in(&self, module_file: ModuleFile<'_>) -> Composed<'_> {
            let file = LinkFile {
                id: FileId::from_raw(0),
                path: &self.path,
                facts: &self.facts,
                imports: &[],
                extra: &self.extra,
            };
            compose(&LinkInput::new(vec![file]), module_file)
        }
    }

    fn keys(composed: &Composed<'_>) -> Vec<String> {
        composed
            .facts
            .iter()
            .map(|linked| linked.fact.key.key().to_owned())
            .collect()
    }

    /// The line and the reason of each cut.
    fn cuts_of(composed: &Composed<'_>) -> Vec<(Line, Why)> {
        composed
            .cuts
            .iter()
            .map(|cut| (cut.line, cut.why))
            .collect()
    }

    /// The line and the detail of each unread site the cuts make.
    fn unread_of(composed: &Composed<'_>) -> Vec<(Line, String)> {
        composed
            .cuts
            .iter()
            .filter_map(Cut::unread)
            .map(|(_, site)| (site.line, site.detail))
            .collect()
    }

    #[test]
    fn routers_that_mount_each_other_end_at_the_first_router_seen_twice() {
        let mut project = Project::new();
        project.declare("a", "/x");
        project.declare("b", "/y");
        project.mount("b", "a", "/ab");
        project.mount("a", "b", "/ba");
        project.mount("app", "a", "/top");
        project.route("a", "/one");
        let composed = project.compose();
        // The way up through `b` comes back to `a`; the way through `app` is whole.
        assert_eq!(keys(&composed), ["GET /top/x/one"]);
        assert_eq!(cuts_of(&composed), [(2, Why::Cycle)]);
    }

    #[test]
    fn routers_that_are_only_mounted_into_each_other_serve_nothing() {
        let mut project = Project::new();
        project.declare("a", "/x");
        project.mount("b", "a", "/ab");
        project.mount("a", "b", "/ba");
        project.route("a", "/one");
        let composed = project.compose();
        // No chain ends at a router nobody mounts, and a path at the router's own prefix would be one nothing serves.
        assert_eq!(keys(&composed), Vec::<String>::new());
        assert!(composed.cuts.iter().all(|cut| cut.why == Why::Cycle));
        assert!(!composed.cuts.is_empty());
    }

    #[test]
    fn a_router_mounted_into_itself_serves_nothing() {
        let mut project = Project::new();
        project.declare("a", "/x");
        project.mount("a", "a", "/again");
        project.route("a", "/one");
        let composed = project.compose();
        assert_eq!(keys(&composed), Vec::<String>::new());
        assert_eq!(composed.cuts.len(), 1);
    }

    #[test]
    fn a_mount_with_a_prefix_that_is_not_a_literal_is_cut_and_the_others_stay() {
        let mut project = Project::new();
        project.declare("r", "/items");
        project.mount("app", "r", "/v2");
        project.mount_reading("app", "r", Reading::Unread, false);
        project.route("r", "/{}");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /v2/items/{}"]);
        assert_eq!(cuts_of(&composed), [(2, Why::MountPrefix)]);
        // `parse` has made the mount an unread site, so the cut is not a second one.
        assert_eq!(unread_of(&composed), []);
    }

    #[test]
    fn no_way_goes_through_a_router_whose_own_prefix_is_not_text() {
        let mut project = Project::new();
        project.declare("child", "/items");
        project.declare_unread("api");
        project.mount("api", "child", "/v2");
        project.mount("app", "api", "/top");
        project.mount("app", "child", "/direct");
        project.route("child", "/{}");
        let composed = project.compose();
        // The way through `api` would lose its prefix; the one that does not pass it is whole. The unread site is the mount of `child` on `api`, the router that holds the prefix.
        assert_eq!(keys(&composed), ["GET /direct/items/{}"]);
        assert_eq!(cuts_of(&composed), [(1, Why::OwnPrefix)]);
        assert_eq!(unread_of(&composed), [(1, "api.include_router".to_owned())]);
    }

    #[test]
    fn a_registration_prefix_stands_in_for_an_own_prefix_that_is_not_text() {
        let mut project = Project::new();
        project.declare_unread("bp");
        project.register("app", "bp", Reading::Text("/api".to_owned()));
        project.route("bp", "/x");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /api/x"]);
        assert_eq!(cuts_of(&composed), []);
    }

    #[test]
    fn a_blueprint_registered_with_no_prefix_keeps_the_own_prefix_that_is_not_text() {
        let mut project = Project::new();
        project.declare_unread("bp");
        project.register("app", "bp", Reading::Absent);
        project.route("bp", "/x");
        let composed = project.compose();
        // The own prefix is the path before `/x`, and nobody knows it: the route is an unread site of its own line.
        assert_eq!(keys(&composed), Vec::<String>::new());
        assert_eq!(cuts_of(&composed), [(1, Why::OwnPrefix)]);
        assert_eq!(unread_of(&composed), [(1, "bp.get".to_owned())]);
    }

    #[test]
    fn a_router_nobody_mounts_whose_own_prefix_is_not_text_serves_nothing() {
        let mut project = Project::new();
        project.declare_unread("r");
        project.route("r", "/x");
        let composed = project.compose();
        assert_eq!(keys(&composed), Vec::<String>::new());
        assert_eq!(unread_of(&composed), [(1, "r.get".to_owned())]);
    }

    #[test]
    fn a_route_is_served_by_the_registration_that_gives_a_prefix_and_unread_for_the_other() {
        let mut project = Project::new();
        project.declare_unread("bp");
        project.register("app", "bp", Reading::Text("/api".to_owned()));
        project.register("other", "bp", Reading::Absent);
        project.route("bp", "/x");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /api/x"]);
        assert_eq!(unread_of(&composed), [(1, "bp.get".to_owned())]);
    }

    #[test]
    fn a_nested_blueprint_is_served_behind_a_registration_prefix_that_replaces_its_parents_own() {
        let mut project = Project::new();
        project.declare("child", "/c");
        project.declare_unread("parent");
        project.register("parent", "child", Reading::Absent);
        project.register("app", "parent", Reading::Text("/api".to_owned()));
        project.route("child", "/x");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /api/c/x"]);
        assert_eq!(cuts_of(&composed), []);
    }

    #[test]
    fn a_nested_blueprint_under_a_parent_with_an_own_prefix_that_is_not_text_is_cut_at_the_mount() {
        let mut project = Project::new();
        project.declare("child", "/c");
        project.declare_unread("parent");
        project.register("parent", "child", Reading::Absent);
        project.register("app", "parent", Reading::Absent);
        project.route("child", "/x");
        let composed = project.compose();
        assert_eq!(keys(&composed), Vec::<String>::new());
        // The unread site is where the child is mounted on the parent that holds the prefix, not where the parent is mounted.
        assert_eq!(cuts_of(&composed), [(1, Why::OwnPrefix)]);
        assert_eq!(
            unread_of(&composed),
            [(1, "parent.include_router".to_owned())]
        );
    }

    #[test]
    fn a_way_that_meets_a_bound_is_an_unread_site_that_names_the_bound() {
        let mut project = Project::new();
        project.mount("b", "a", "/ab");
        project.mount("a", "b", "/ba");
        project.route("a", "/one");
        let composed = project.compose();
        let reasons: Vec<UnresolvedReason> = composed
            .cuts
            .iter()
            .filter_map(Cut::unread)
            .map(|(_, site)| site.reason)
            .collect();
        assert!(matches!(
            reasons.as_slice(),
            [UnresolvedReason::Unsupported(bound)] if bound.contains("include each other")
        ));
    }

    /// `count` layers of two routers, each router mounted into both routers of the layer above, so that `a0` is reached in 2^`count` ways and has the route `/leaf`. The mounts are written from the lowest layer up, or the other way round.
    fn layers(count: usize, reversed: bool) -> Project {
        let mut mounts = Vec::new();
        for layer in 0..count {
            for from in ["a", "b"] {
                for to in ["a", "b"] {
                    mounts.push((
                        format!("{to}{}", layer + 1),
                        format!("{from}{layer}"),
                        format!("/{layer}{from}{to}"),
                    ));
                }
            }
        }
        if reversed {
            mounts.reverse();
        }
        let mut project = Project::new();
        for (parent, child, prefix) in &mounts {
            project.mount(parent, child, prefix);
        }
        project.route("a0", "/leaf");
        project
    }

    #[test]
    fn the_ways_up_from_one_router_are_capped() {
        let project = layers(7, false);
        let composed = project.compose();
        assert_eq!(composed.facts.len(), MAX_WAYS);
        assert!(!composed.cuts.is_empty());
        assert!(composed.cuts.iter().all(|cut| cut.why == Why::TooMany));
    }

    #[test]
    fn the_ways_kept_are_the_first_by_path_whatever_the_order_of_the_mounts() {
        // A way is the choice of one of the two routers at each layer, the lowest layer in the lowest bit.
        let letter = |choice: u32, layer: usize| if (choice >> layer) & 1 == 0 { 'a' } else { 'b' };
        let mut every: Vec<String> = (0..1_u32 << 7)
            .map(|choice| {
                let mut path = String::new();
                for layer in (0..7).rev() {
                    let from = if layer == 0 {
                        'a'
                    } else {
                        letter(choice, layer - 1)
                    };
                    write!(path, "/{layer}{from}{}", letter(choice, layer)).unwrap();
                }
                format!("GET {path}/leaf")
            })
            .collect();
        every.sort();
        every.truncate(MAX_WAYS);
        let (straight, reversed) = (layers(7, false), layers(7, true));
        assert_eq!(keys(&straight.compose()), every);
        assert_eq!(keys(&reversed.compose()), every);
    }

    #[test]
    fn the_walk_up_from_one_router_stops_at_its_bound_whatever_the_order_of_the_mounts() {
        // 2^11 ways up are more than the walk follows.
        let (straight, reversed) = (layers(11, false), layers(11, true));
        let (straight, reversed) = (straight.compose(), reversed.compose());
        assert_eq!(straight.facts.len(), MAX_WAYS);
        assert!(straight.cuts.iter().all(|cut| cut.why == Why::TooMany));
        assert_eq!(keys(&straight), keys(&reversed));
    }

    #[test]
    fn a_chain_of_mounts_deeper_than_the_limit_is_cut() {
        let mut project = Project::new();
        for level in 0..=MAX_DEPTH + 8 {
            project.mount(&format!("r{}", level + 1), &format!("r{level}"), "/up");
        }
        project.declare("r0", "/own");
        project.route("r0", "/leaf");
        let composed = project.compose();
        assert_eq!(keys(&composed), Vec::<String>::new());
        assert_eq!(composed.cuts.len(), 1);
        assert!(composed.cuts.iter().all(|cut| cut.why == Why::TooDeep));
    }

    #[test]
    fn a_name_a_module_imports_from_itself_ends_at_the_limit_of_imports() {
        let mut project = Project::new();
        // `from app import r` in `app.py`, which every specifier names: the import leads back to the same name.
        project.extra.binds.push(Bind {
            local: "r".to_owned(),
            spec: "app".to_owned(),
            line: 1,
            name: Some("r".to_owned()),
        });
        project.mount("app", "r", "/x");
        project.route("r", "/one");
        let composed = project.compose_in(&|_, _| Some(FileId::from_raw(0)));
        // The router is never found, so the mount reaches nothing and the route is served on its own.
        assert_eq!(keys(&composed), ["GET /one"]);
    }

    #[test]
    fn a_replacing_mount_drops_the_routers_own_prefix() {
        let mut project = Project::new();
        project.declare("bp", "/bookings");
        project.register("app", "bp", Reading::Text("/api".to_owned()));
        project.route("bp", "/{}");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /api/{}"]);
    }

    #[test]
    fn a_mount_without_a_prefix_serves_the_route_once() {
        let mut project = Project::new();
        project.declare("r", "/items");
        project.mount_reading("app", "r", Reading::Absent, false);
        project.route("r", "/{}");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /items/{}"]);
    }
}
