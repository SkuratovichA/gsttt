//! The cross-file half of routing. `parse` records, for every file, the routes declared on routers, the prefix each
//! router declares for itself and the routers the file mounts under a prefix; here the mounts are followed from the
//! router of each route up to the routers nobody mounts, and the route is served once for every way up, behind the
//! prefixes of every mount and router on the way.
//!
//! A router's routes are no facts of its file, so the path a router serves on its own appears only when no mount
//! reaches it. A mount names its router by an expression of its own file, and a package that re-exports the router
//! under another name is followed to the module that makes it.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::contract::ContractKind;
use gsttt_core::fact::{ContractFact, Line};
use gsttt_core::ids::FileId;
use gsttt_core::path::RepoPath;
use gsttt_core::rules::contract_key;

use crate::extra::{Mount, PyExtra, Target};
use crate::project::PythonProject;
use crate::resolve::module_file;
use crate::site::join_prefixes;

/// How many mounts are followed upward from one router, so that a very deep tree of mounts cannot exhaust the
/// stack. Routers that mount each other end sooner, at the first router seen twice.
pub(crate) const MAX_DEPTH: usize = 32;

/// How many ways up are followed from one router, whole or cut. Mounts shared at every level multiply the ways, and
/// a router mounted in dozens of places is unlike any real application.
pub(crate) const MAX_WAYS: usize = 64;

/// How many imports are followed from the name a mount writes to the module that makes the router. A re-export is one
/// or two imports deep, and modules that import each other's names end at the bound.
const MAX_HOPS: usize = 8;

/// A router: the file it lives in and the variable that holds it. Django's `urlpatterns` is the module itself and
/// has the empty name.
type Router<'a> = (FileId, &'a str);

/// Finds the file of the module a specifier names, as the file `from` imports it.
type ModuleFile<'f> = &'f dyn Fn(&RepoPath, &str) -> Option<FileId>;

/// What the names of the files of a project stand for: the files themselves and where their imports lead.
struct Names<'i, 'a> {
    input: &'i LinkInput<'a, PyExtra>,
    module_file: ModuleFile<'i>,
}

/// A mount seen from the router it mounts.
struct Parent<'a> {
    /// The router the mount is called on: `app` in `app.include_router(…)`.
    router: Router<'a>,
    prefix: Option<&'a str>,
    replaces: bool,
    /// Whether the mount gives a prefix that is not a literal.
    unread_prefix: bool,
    /// Where the mount is, for a cut way to name.
    file: FileId,
    line: Line,
}

/// Why a way up from a router was not followed to its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Why {
    /// The way came back to a router it had passed: routers mounted into each other.
    Cycle,
    TooDeep,
    TooMany,
    /// The mount gives a prefix that is not a literal, so the path behind it is not known.
    Prefix,
}

/// A mount that a way up from a router stopped at. Coverage should count it as a site `link` could not read.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cut<'a> {
    file: FileId,
    line: Line,
    /// The router the mount mounts.
    router: &'a str,
    why: Why,
}

/// The mounts of one project, read from the `extra` of its files.
struct Mounts<'a> {
    /// The mounts by the router they mount.
    parents: BTreeMap<Router<'a>, Vec<Parent<'a>>>,
    /// The prefix each router declares for itself.
    own: BTreeMap<Router<'a>, &'a str>,
}

/// What `link` made, and the ways it could not follow.
struct Composed<'a> {
    facts: Vec<LinkedFact>,
    cuts: BTreeSet<Cut<'a>>,
}

/// The routes declared on routers, once for each way their router is mounted, behind the prefixes of the mounts and
/// of the routers above it.
pub(crate) fn link(project: &PythonProject, input: &LinkInput<'_, PyExtra>) -> Vec<LinkedFact> {
    let Composed { facts, cuts } = compose(input, &|from, spec| module_file(project, from, spec));
    // `Project::link` returns facts only, so a way that was cut is told to the log until coverage can take it.
    for cut in &cuts {
        let file = input.get(cut.file).map_or("", |file| file.path.as_str());
        tracing::warn!(
            file,
            line = cut.line,
            router = cut.router,
            why = ?cut.why,
            "the mounts above a router were not followed to the end"
        );
    }
    facts
}

fn compose<'a>(input: &LinkInput<'a, PyExtra>, module_file: ModuleFile<'_>) -> Composed<'a> {
    let mounts = Mounts::read(&Names { input, module_file });
    let mut cuts = BTreeSet::new();
    // The ways up depend on the router alone, so every route of a router shares them.
    let mut ways: BTreeMap<Router<'a>, Vec<String>> = BTreeMap::new();
    let mut facts = Vec::new();
    for file in input.files().iter().copied() {
        for route in &file.extra.routes {
            let Some((method, path)) = route.fact.key.http_parts() else {
                continue;
            };
            let router = (file.id, route.var.as_str());
            let prefixes = ways
                .entry(router)
                .or_insert_with(|| mounts.prefixes(router, &mut cuts));
            for prefix in prefixes.iter() {
                facts.push(LinkedFact {
                    file: file.id,
                    fact: ContractFact {
                        key: contract_key(&ContractKind::Http, Some(prefix), path, Some(method)),
                        role: route.fact.role,
                        line: route.fact.line,
                        via: route.fact.via.clone(),
                    },
                });
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
                        prefix: mount.prefix.as_deref(),
                        replaces: mount.replaces,
                        unread_prefix: mount.unread_prefix,
                        file: file.id,
                        line: mount.line,
                    });
                }
            }
            for router in &file.extra.routers {
                own.insert((file.id, router.var.as_str()), router.prefix.as_str());
            }
        }
        Self { parents, own }
    }

    fn own_prefix(&self, router: Router<'a>) -> &'a str {
        self.own.get(&router).copied().unwrap_or_default()
    }

    /// Every prefix `start` is served behind, sorted: one for each way up through the mounts to a router nobody
    /// mounts. A router that no mount reaches is served behind its own prefix. One whose every way up is cut is
    /// mounted, by a mount that cannot be followed to its end, so it is served behind no prefix that is known.
    fn prefixes(&self, start: Router<'a>, cuts: &mut BTreeSet<Cut<'a>>) -> Vec<String> {
        let mut climb = Climb {
            mounts: self,
            ways: BTreeSet::new(),
            ended: 0,
            cuts,
        };
        climb.up(start, "", &mut vec![start]);
        climb.ways.into_iter().collect()
    }
}

/// One climb from a router to the routers above it.
struct Climb<'m, 'a, 'c> {
    mounts: &'m Mounts<'a>,
    /// The prefix of each way that reached a router nobody mounts.
    ways: BTreeSet<String>,
    /// Ways ended so far, whole or cut.
    ended: usize,
    cuts: &'c mut BTreeSet<Cut<'a>>,
}

impl<'a> Climb<'_, 'a, '_> {
    /// Climbs from `node`. `below` is the path between `node` and the route: everything the routers and mounts
    /// below `node` put in front of it, except `node`'s own prefix, which a replacing mount may drop. `trail` holds
    /// the routers passed, `node` last.
    fn up(&mut self, node: Router<'a>, below: &str, trail: &mut Vec<Router<'a>>) {
        let own = self.mounts.own_prefix(node);
        let Some(parents) = self.mounts.parents.get(&node) else {
            self.ended += 1;
            self.ways
                .insert(join_prefixes(&ContractKind::Http, own, below));
            return;
        };
        for parent in parents {
            let why = if self.ended >= MAX_WAYS {
                Some(Why::TooMany)
            } else if trail.contains(&parent.router) {
                Some(Why::Cycle)
            } else if trail.len() >= MAX_DEPTH {
                Some(Why::TooDeep)
            } else if parent.unread_prefix {
                Some(Why::Prefix)
            } else {
                None
            };
            if let Some(why) = why {
                self.ended += 1;
                self.cuts.insert(Cut {
                    file: parent.file,
                    line: parent.line,
                    router: node.1,
                    why,
                });
                continue;
            }
            // FastAPI and Django put the mount's prefix in front of the router's own; Flask's registration prefix
            // stands where the blueprint's own would have been, and the blueprint keeps its own only without one.
            let local = match (parent.replaces, parent.prefix) {
                (true, prefix) => prefix.unwrap_or(own).to_owned(),
                (false, prefix) => {
                    join_prefixes(&ContractKind::Http, prefix.unwrap_or_default(), own)
                }
            };
            let below = join_prefixes(&ContractKind::Http, &local, below);
            trail.push(parent.router);
            self.up(parent.router, &below, trail);
            trail.pop();
        }
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

    /// The router an expression names in `file`: a variable of the file itself, or a name its imports bind. A module
    /// that holds the name only by importing it from another, as a package does that re-exports a router, is
    /// followed on to the module that makes it; `hops` counts the imports followed so far.
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
            // `from app.api.bookings import router`: the module holds the router, even when it is also called
            // `router`.
            (Some(name), "") => ((self.module_file)(file.path, &bind.spec)?, name),
            // `from app.api import bookings; bookings.router`: the name is a module of the package, and the router
            // is inside it.
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

/// The specifier of the module `name` inside the package `spec`: `app.api` and `bookings` give `app.api.bookings`,
/// and `.` gives `.bookings`.
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
            let line = Line::try_from(self.extra.mounts.len() + 1).unwrap();
            self.extra.mounts.push(Mount {
                line,
                receiver: parent.to_owned(),
                target: Target::Name(child.to_owned()),
                prefix: Some(prefix.to_owned()),
                unread_prefix: false,
                replaces: false,
            });
        }

        fn declare(&mut self, router: &str, prefix: &str) {
            self.extra.routers.push(RouterPrefix {
                var: router.to_owned(),
                prefix: prefix.to_owned(),
            });
        }

        fn route(&mut self, router: &str, path: &str) {
            self.extra.routes.push(Route {
                var: router.to_owned(),
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
        let cuts: Vec<(&str, Why)> = composed
            .cuts
            .iter()
            .map(|cut| (cut.router, cut.why))
            .collect();
        assert_eq!(cuts, [("b", Why::Cycle)]);
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
        project.extra.mounts.push(Mount {
            line: 7,
            receiver: "app".to_owned(),
            target: Target::Name("r".to_owned()),
            prefix: None,
            unread_prefix: true,
            replaces: false,
        });
        project.route("r", "/{}");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /v2/items/{}"]);
        let cuts: Vec<(Line, Why)> = composed
            .cuts
            .iter()
            .map(|cut| (cut.line, cut.why))
            .collect();
        assert_eq!(cuts, [(7, Why::Prefix)]);
    }

    #[test]
    fn the_ways_up_from_one_router_are_capped() {
        // Seven layers of two routers, each router mounted into both routers of the layer above: 2^7 ways up.
        let mut project = Project::new();
        for layer in 0..7 {
            for from in ["a", "b"] {
                for to in ["a", "b"] {
                    project.mount(
                        &format!("{to}{}", layer + 1),
                        &format!("{from}{layer}"),
                        &format!("/{layer}{from}{to}"),
                    );
                }
            }
        }
        project.route("a0", "/leaf");
        let composed = project.compose();
        assert_eq!(composed.facts.len(), MAX_WAYS);
        assert!(!composed.cuts.is_empty());
        assert!(composed.cuts.iter().all(|cut| cut.why == Why::TooMany));
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
        project.mount("app", "bp", "/api");
        project
            .extra
            .mounts
            .iter_mut()
            .for_each(|mount| mount.replaces = true);
        project.route("bp", "/{}");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /api/{}"]);
    }

    #[test]
    fn a_mount_without_a_prefix_serves_the_route_once() {
        let mut project = Project::new();
        project.declare("r", "/items");
        project.extra.mounts.push(Mount {
            line: 1,
            receiver: "app".to_owned(),
            target: Target::Name("r".to_owned()),
            prefix: None,
            unread_prefix: false,
            replaces: false,
        });
        project.route("r", "/{}");
        let composed = project.compose();
        assert_eq!(keys(&composed), ["GET /items/{}"]);
    }
}
