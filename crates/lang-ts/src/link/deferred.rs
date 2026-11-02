//! The sites the parse left to the link: those whose rule needed the value of a constant that another file declares, and the calls on a client or a router that another file makes, which are read on what that file makes under the name, or on what this file makes with a name that another file declares.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};

use gsttt_core::FileId;
use gsttt_core::connector::{LinkFile, LinkInput, LinkedFact};
use gsttt_core::fact::{ContractFact, UnresolvedSite};
use gsttt_core::rules::{CallRule, Outcome, Pack, serves_http};

use super::{Node, through};
use crate::extra::{ClassRoute, Deferred, DeferredTarget, LinkMaker, Route, RuleRef, TsExtra};
use crate::parts::Part;
use crate::shape::Shape;

/// What the deferred sites of every file came to, once the constants they name were read and the clients and routers they are called on were found.
#[derive(Default)]
pub(super) struct Resolved {
    /// Contracts of the files that declare no host or class for them, which the link finds on its own.
    pub(super) own: Vec<LinkedFact>,
    /// Routes served by a variable, by file, which wait for the prefixes its mounts give it.
    routes: BTreeMap<FileId, Vec<Route>>,
    /// Routes that a decorator declares, by file, which wait for the prefixes of the modules that reach the class.
    class_routes: BTreeMap<FileId, Vec<ClassRoute>>,
    /// Routes that a file serves on an app or router another file makes, by the file that registers the route, the router as its own file knows it, and the route as the rules found it, which wait for the prefixes the mounts of the router give it.
    pub(super) served: Vec<(FileId, Node, ContractFact)>,
    /// The sites whose rule still could not read them.
    pub(super) unread: Vec<(FileId, UnresolvedSite)>,
}

impl Resolved {
    /// The routes of `file` that were left to the link.
    pub(super) fn routes_of(&self, file: FileId) -> &[Route] {
        self.routes.get(&file).map_or(&[], Vec::as_slice)
    }

    /// The class routes of `file` that were left to the link.
    pub(super) fn class_routes_of(&self, file: FileId) -> &[ClassRoute] {
        self.class_routes.get(&file).map_or(&[], Vec::as_slice)
    }
}

/// Reads the constants that the deferred sites of every file name, finds what the files that make a client or a router a site is called on make of it, and applies the rule of each site again.
pub(super) fn resolve(input: &LinkInput<'_, TsExtra>, packs: &[Pack]) -> Resolved {
    let mut resolved = Resolved::default();
    let reader = Reader::new(input);
    for file in input.files() {
        for deferred in &file.extra.deferred {
            let mut shape = deferred.shape.clone();
            shape.fill(|name| reader.constant(file, name));
            let (outcome, served) = match &deferred.target {
                DeferredTarget::Receiver { host } => {
                    match on_made(&reader, packs, file, deferred, host, &shape) {
                        Some(read) => read,
                        None => continue,
                    }
                }
                DeferredTarget::Made { host } => {
                    match on_own(&reader, packs, file, deferred, host, &shape) {
                        Some(read) => read,
                        None => continue,
                    }
                }
                // A client that another file makes is read by the rules that take a receiver, so the rule that takes none does not read the call.
                DeferredTarget::Fallback { host, rivals }
                    if made_for(input, packs, file, host, rivals) =>
                {
                    continue;
                }
                _ => match apply(packs, deferred, deferred.prefix.as_deref(), &shape) {
                    Some(outcome) => (outcome, None),
                    None => continue,
                },
            };
            match outcome {
                Outcome::Fact(fact) => match (&deferred.target, served) {
                    (_, Some(node)) => resolved.served.push((file.id, node, fact)),
                    (DeferredTarget::Host(host), None) => {
                        resolved.routes.entry(file.id).or_default().push(Route {
                            host: host.clone(),
                            fact,
                        });
                    }
                    (DeferredTarget::Class { class, inherited }, None) => {
                        resolved
                            .class_routes
                            .entry(file.id)
                            .or_default()
                            .push(ClassRoute {
                                class: class.clone(),
                                fact,
                                inherited: *inherited,
                            });
                    }
                    (
                        DeferredTarget::Own
                        | DeferredTarget::Receiver { .. }
                        | DeferredTarget::Made { .. }
                        | DeferredTarget::Fallback { .. },
                        None,
                    ) => resolved.own.push(LinkedFact::found(file.id, fact)),
                },
                Outcome::Blind(site) => resolved.unread.push((file.id, site)),
                Outcome::Skip => {}
            }
        }
    }
    resolved
}

/// The call rule at `at` and the pack it is in. `None` when the packs have no such rule, which the cache keeps from happening because it is keyed by the packs.
fn call_rule(packs: &[Pack], at: RuleRef) -> Option<(&Pack, &CallRule)> {
    match at {
        RuleRef::Call { pack, rule } => {
            let pack = packs.get(pack)?;
            Some((pack, pack.calls.get(rule)?))
        }
        RuleRef::Decorator { .. } => None,
    }
}

/// What the rule that read `deferred` makes of `shape`, given `prefix`. `None` when the packs have no such rule.
fn apply(
    packs: &[Pack],
    deferred: &Deferred,
    prefix: Option<&str>,
    shape: &Shape,
) -> Option<Outcome> {
    if let Some((pack, rule)) = call_rule(packs, deferred.rule) {
        return Some(rule.apply(
            &pack.name,
            pack.relative_without_base,
            shape,
            prefix,
            deferred.line,
            &deferred.what,
        ));
    }
    let RuleRef::Decorator { pack, rule } = deferred.rule else {
        return None;
    };
    let pack = packs.get(pack)?;
    Some(
        pack.decorators
            .get(rule)?
            .apply(&pack.name, shape, prefix, deferred.line, &deferred.what),
    )
}

/// The variable that `host`, written in `file`, is imported as, when the file it leads to makes it with a call that some rule takes a receiver from, and the node it is in that file.
pub(super) fn maker_of<'a>(
    input: &LinkInput<'a, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    host: &str,
) -> Option<(Node, &'a LinkMaker)> {
    // A member of the variable (`api.v1`) is not the variable.
    let (node, None) = imported(input, file, host)? else {
        return None;
    };
    let maker = made_in(input.get(node.0)?.extra, &node.1)?;
    Some((node, maker))
}

/// The variable `name`, which the file that `extra` is of makes with a call that some rule takes a receiver from.
fn made_in<'a>(extra: &'a TsExtra, name: &str) -> Option<&'a LinkMaker> {
    let found = extra
        .makers
        .binary_search_by(|maker| maker.name.as_str().cmp(name))
        .ok()?;
    extra.makers.get(found)
}

/// What the rule of a call on `host` makes of the call when `host`, written in `file`, is a client or a router that another file makes: the prefix is the one the rule takes from the call that made it, read against the constants of the file that made it, and the call is a blind spot when a rule that serves routes finds a prefix there that is not text. The node is the router the route is served on, for a rule that serves routes. `None` when `host` is no such variable, or the rule takes no receiver from the call that made it.
fn on_made(
    reader: &Reader<'_, '_>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    deferred: &Deferred,
    host: &str,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let (node, maker) = maker_of(reader.input, file, host)?;
    let owner = reader.input.get(node.0)?;
    on_maker(reader, packs, owner, node, maker, deferred, shape)
}

/// What the rule of a call on `host` makes of the call when `host` is a client or a router that `file` makes itself, with a call that names something another file declares. `None` when the file keeps no such variable, or the rule takes no receiver from the call that made it.
fn on_own(
    reader: &Reader<'_, '_>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    deferred: &Deferred,
    host: &str,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let maker = made_in(file.extra, host)?;
    on_maker(
        reader,
        packs,
        file,
        (file.id, host.to_owned()),
        maker,
        deferred,
        shape,
    )
}

/// What the rule of `deferred` makes of the call `shape` on the variable `maker` stands for, which `owner` declares as `node`.
fn on_maker(
    reader: &Reader<'_, '_>,
    packs: &[Pack],
    owner: &LinkFile<'_, TsExtra>,
    node: Node,
    maker: &LinkMaker,
    deferred: &Deferred,
    shape: &Shape,
) -> Option<(Outcome, Option<Node>)> {
    let (pack, rule) = call_rule(packs, deferred.rule)?;
    let receiver = rule.receiver.as_ref()?;
    if !receiver.from.matches(&maker.callee) {
        return None;
    }
    let mut made = maker.shape.clone();
    made.fill(|name| reader.constant(owner, name));
    let reading = receiver.reading(&made);
    let Ok(prefix) = rule.behind(&reading) else {
        return Some((Outcome::blind(deferred.line, &deferred.what), None));
    };
    let outcome = rule.apply(
        &pack.name,
        pack.relative_without_base,
        shape,
        prefix,
        deferred.line,
        &deferred.what,
    );
    Some((outcome, serves_http(&rule.kind, rule.role).then_some(node)))
}

/// Whether `host`, written in `file`, is a variable that another file makes with a call that some rule of `rivals` takes a receiver from.
fn made_for(
    input: &LinkInput<'_, TsExtra>,
    packs: &[Pack],
    file: &LinkFile<'_, TsExtra>,
    host: &str,
    rivals: &[RuleRef],
) -> bool {
    let Some((_, maker)) = maker_of(input, file, host) else {
        return false;
    };
    rivals.iter().any(|rival| {
        call_rule(packs, *rival)
            .and_then(|(_, rule)| rule.receiver.as_ref())
            .is_some_and(|receiver| receiver.from.matches(&maker.callee))
    })
}

/// The export that `name`, written in `file`, reaches when its first segment is a name the file imports: the node where it is defined, and the rest of the name below it. A namespace names the export in the segment after it.
fn imported<'n>(
    input: &LinkInput<'_, TsExtra>,
    file: &LinkFile<'_, TsExtra>,
    name: &'n str,
) -> Option<(Node, Option<&'n str>)> {
    let (root, rest) = match name.split_once('.') {
        Some((root, rest)) => (root, Some(rest)),
        None => (name, None),
    };
    let binding = file
        .extra
        .bindings
        .iter()
        .find(|binding| binding.local == root)?;
    let (export, path) = if binding.name == "*" {
        let rest = rest?;
        match rest.split_once('.') {
            Some((export, path)) => (export, Some(path)),
            None => (rest, None),
        }
    } else {
        (binding.name.as_str(), rest)
    };
    Some((through(input, file, binding, export, 0)?, path))
}

/// A stored constant that is being read: where it is written, what it is built from, and how many of the names it is built from have been taken up.
struct Frame<'a, 'i> {
    node: Node,
    owner: &'a LinkFile<'i, TsExtra>,
    value: &'i Part,
    names: Vec<&'i str>,
    next: usize,
}

/// The most text the link keeps for the constants it has read, in all. Each constant keeps the whole text it reads as, so a chain of n constants that each add to the one before would hold the sum of their lengths, and a long run of constants that each read as a text near the limit of one would hold that many copies of it. A constant that would take the link past this reads as none.
const MAX_KEPT: usize = 32 << 20;

/// What the link reads of the constants of the files, and the text each constant read as once it was read. A constant that many others name is read once, not once for every way to it: a stored constant reads as text only when everything it names does, so a constant that names one on the way to itself reads as none whichever of them the read starts from, and what a constant reads as does not depend on the constants it is read inside of. A chain of constants is as long as the files are big, so the read keeps its own stack of the constants it is inside of and never calls itself.
struct Reader<'a, 'i> {
    input: &'a LinkInput<'i, TsExtra>,
    read: RefCell<HashMap<Node, Option<String>>>,
    /// The bytes of text in `read`.
    kept: Cell<usize>,
}

impl<'a, 'i> Reader<'a, 'i> {
    fn new(input: &'a LinkInput<'i, TsExtra>) -> Self {
        Self {
            input,
            read: RefCell::default(),
            kept: Cell::new(0),
        }
    }

    /// Counts the text of a constant that was read against what the link keeps, and says whether it fits.
    fn keep(&self, text: &str) -> bool {
        let kept = self.kept.get().saturating_add(text.len());
        let fits = kept <= MAX_KEPT;
        if fits {
            self.kept.set(kept);
        }
        fits
    }

    /// The text that `name`, written in `file`, stands for in the file it is imported from: `LOGIN_PATH` taken by name, `Env.API_URL` for a member, `ROUTES.users.byId` for a nested property, and `paths.LOGIN_PATH` for a constant taken through a namespace. A constant built from names that other files declare is read in the file that declares it, and only when it reads whole: one that comes back to itself has no text.
    ///
    /// The constants a read needs are read first, one at a time, each on top of the one that needs it. A name that is read already, or is one of the constants the read is inside of, is not read again; when none is left, the constant on top reads its own part with the names that were read, and a name it is inside of reads as none. Only a name that is really on the way down is inside, not one that waits for its turn among the names of a constant: a constant that names two which both name a third is no cycle.
    fn constant(&self, file: &LinkFile<'_, TsExtra>, name: &str) -> Option<String> {
        let root = self.node(file, name)?;
        if let Some(read) = self.read.borrow().get(&root) {
            return read.clone();
        }
        let mut path = Vec::new();
        let mut inside = HashSet::new();
        self.enter(root.clone(), &mut path, &mut inside);
        while let Some(frame) = path.last_mut() {
            if let Some(name) = frame.names.get(frame.next).copied() {
                frame.next += 1;
                let Some(node) = self.node(frame.owner, name) else {
                    continue;
                };
                if !inside.contains(&node) && !self.read.borrow().contains_key(&node) {
                    self.enter(node, &mut path, &mut inside);
                }
                continue;
            }
            let Some(frame) = path.pop() else {
                break;
            };
            inside.remove(&frame.node);
            let read = frame
                .value
                .read(&|name| {
                    let node = self.node(frame.owner, name)?;
                    self.read.borrow().get(&node)?.clone()
                })
                .filter(|text| self.keep(text));
            self.read.borrow_mut().insert(frame.node, read);
        }
        self.read.borrow().get(&root)?.clone()
    }

    /// The node that `name`, written in `file`, reaches: the file that declares it and the key its constant is kept under there.
    fn node(&self, file: &LinkFile<'_, TsExtra>, name: &str) -> Option<Node> {
        let ((defined_in, local), path) = imported(self.input, file, name)?;
        let key = match path {
            Some(path) => format!("{local}.{path}"),
            None => local,
        };
        Some((defined_in, key))
    }

    /// Puts the constant `node` on top of `path`. A node that no constant is kept under reads as none, for good.
    fn enter(&self, node: Node, path: &mut Vec<Frame<'a, 'i>>, inside: &mut HashSet<Node>) {
        let stored = self.input.get(node.0).and_then(|owner| {
            let constants = &owner.extra.constants;
            let found = constants
                .binary_search_by(|(known, _)| known.as_str().cmp(&node.1))
                .ok()?;
            let (_, value) = constants.get(found)?;
            Some((owner, value))
        });
        let Some((owner, value)) = stored else {
            self.read.borrow_mut().insert(node, None);
            return;
        };
        inside.insert(node.clone());
        path.push(Frame {
            node,
            owner,
            value,
            names: value.names(),
            next: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::thread;

    use gsttt_core::connector::{LinkFile, LinkInput};
    use gsttt_core::fact::{FileFacts, Import, ImportKind, Resolution, ResolvedImport};
    use gsttt_core::ids::FileId;
    use gsttt_core::path::RepoPath;

    use super::{MAX_KEPT, Reader};
    use crate::extra::{Binding, Export, ExportTarget, TsExtra};
    use crate::parts::{MAX_COMPOSED, Part, Reads};

    /// The stack of the thread that reads a chain of constants. A read that called itself once for every constant of a chain this long would overflow it long before the end.
    const SMALL_STACK: usize = 256 * 1024;

    /// A constant that is its name's `Name` followed by `/y`, as a stored constant is.
    fn then_y(next: &str) -> Part {
        Part::Cat(
            Reads::All,
            vec![Part::Name(next.to_owned()), Part::Text("/y".to_owned())],
        )
    }

    fn text(text: &str) -> Part {
        Part::Text(text.to_owned())
    }

    /// What the user reads each of `names` as, in order, with one reader for all of them. Every constant is exported by the file of `files` that keeps it, and every file takes the names its constants are built from from the file that keeps them. The user is one more file, which keeps nothing and takes `names`.
    fn user_reads(files: &[Vec<(&str, Part)>], names: &[&str]) -> Vec<Option<String>> {
        let home: HashMap<&str, usize> = files
            .iter()
            .enumerate()
            .flat_map(|(file, constants)| constants.iter().map(move |(name, _)| (*name, file)))
            .collect();
        let mut sources: Vec<(TsExtra, Vec<ResolvedImport>)> = Vec::new();
        for file in 0..=files.len() {
            let mut extra = TsExtra::default();
            let mut taken = names.to_vec();
            taken.retain(|_| file == files.len());
            for (name, part) in files.get(file).into_iter().flatten() {
                extra.exports.push(Export {
                    name: (*name).to_owned(),
                    target: ExportTarget::Local((*name).to_owned()),
                });
                extra.constants.push(((*name).to_owned(), part.clone()));
                taken.extend(part.names());
            }
            extra.constants.sort_by(|(a, _), (b, _)| a.cmp(b));
            taken.sort_unstable();
            taken.dedup();
            let mut imports = Vec::new();
            for name in taken {
                let Some(&from) = home.get(name) else {
                    continue;
                };
                let spec = format!("m{from}");
                extra.bindings.push(Binding {
                    local: name.to_owned(),
                    spec: spec.clone(),
                    name: name.to_owned(),
                });
                if imports
                    .iter()
                    .all(|known: &ResolvedImport| known.import.spec != spec)
                {
                    imports.push(ResolvedImport {
                        import: Import::new(spec, ImportKind::Static, 1),
                        to: Resolution::File {
                            file: FileId::from_raw(u32::try_from(from).unwrap()),
                        },
                    });
                }
            }
            sources.push((extra, imports));
        }
        let location = RepoPath::normalize("src/main.ts").unwrap();
        let facts = FileFacts::default();
        let input = LinkInput::new(
            sources
                .iter()
                .enumerate()
                .map(|(index, (extra, imports))| LinkFile {
                    id: FileId::from_raw(u32::try_from(index).unwrap()),
                    path: &location,
                    facts: &facts,
                    imports,
                    extra,
                })
                .collect(),
        );
        let reader = Reader::new(&input);
        let user = input
            .get(FileId::from_raw(u32::try_from(files.len()).unwrap()))
            .unwrap();
        names
            .iter()
            .map(|name| reader.constant(user, name))
            .collect()
    }

    #[test]
    fn a_constant_that_two_constants_name_is_no_cycle() {
        // `A` names `B` and `C`, and `C` names `B`: `B` waits for its turn among the names of `A` while `C` reads it.
        let files = [
            vec![(
                "A",
                Part::Cat(
                    Reads::All,
                    vec![
                        Part::Name("B".to_owned()),
                        text("-"),
                        Part::Name("C".to_owned()),
                    ],
                ),
            )],
            vec![("B", text("b"))],
            vec![("C", then_y("B"))],
        ];
        assert_eq!(
            user_reads(&files, &["A", "B", "C"]),
            [
                Some("b-b/y".to_owned()),
                Some("b".to_owned()),
                Some("b/y".to_owned())
            ]
        );
    }

    #[test]
    fn a_cycle_of_constants_reads_as_none_from_either_end_and_from_outside() {
        let files = [
            vec![("A", then_y("B"))],
            vec![("B", then_y("A"))],
            vec![("C", then_y("B"))],
        ];
        for names in [["A", "B", "C"], ["B", "A", "C"], ["C", "B", "A"]] {
            assert_eq!(user_reads(&files, &names), [None, None, None], "{names:?}");
        }
        // A constant that names itself is one too.
        assert_eq!(user_reads(&[vec![("A", then_y("A"))]], &["A"]), [None]);
    }

    #[test]
    fn a_name_that_is_no_constant_reads_as_none_and_the_constants_on_it_with_it() {
        let files = [vec![("A", then_y("B")), ("B", then_y("Missing"))]];
        assert_eq!(
            user_reads(&files, &["A", "B", "Missing"]),
            [None, None, None]
        );
    }

    #[test]
    fn a_chain_of_constants_across_files_is_read_to_its_end_on_a_small_stack() {
        let length = 5000;
        let read = thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn(move || {
                // `K0` is read from the user, and each `K<n>` names `K<n+1>`, which the other of two files keeps.
                let names: Vec<String> = (0..length).map(|number| format!("K{number}")).collect();
                let mut files: [Vec<(&str, Part)>; 2] = [Vec::new(), Vec::new()];
                for (number, name) in names.iter().enumerate() {
                    let part = names
                        .get(number + 1)
                        .map_or_else(|| text("/end"), |next| then_y(next));
                    files[number % 2].push((name, part));
                }
                user_reads(&files, &["K0", "K1"])
            })
            .unwrap()
            .join()
            .unwrap();
        let from = |number: usize| format!("/end{}", "/y".repeat(length - 1 - number));
        assert_eq!(read, [Some(from(0)), Some(from(1))]);
    }

    #[test]
    fn the_constants_that_would_make_the_link_keep_too_much_text_read_as_none() {
        // `P0` is a text of the longest a constant may be, and each `P<n>` names the one before and adds nothing, so each one reads as that whole text again.
        let fitting = MAX_KEPT / MAX_COMPOSED;
        let names: Vec<String> = (0..fitting + 6)
            .map(|number| format!("P{number}"))
            .collect();
        let mut files: [Vec<(&str, Part)>; 2] = [Vec::new(), Vec::new()];
        let mut before: Option<&String> = None;
        for (number, name) in names.iter().enumerate() {
            let part = before.map_or_else(
                || text(&"x".repeat(MAX_COMPOSED)),
                |before| Part::Cat(Reads::All, vec![Part::Name(before.clone())]),
            );
            files[number % 2].push((name, part));
            before = Some(name);
        }
        let last_to_fit = format!("P{}", fitting - 1);
        let first_past = format!("P{fitting}");
        let last = format!("P{}", fitting + 5);
        let read = user_reads(&files, &[&last_to_fit, &first_past, "P1", &last]);
        // The first read keeps the text of every constant up to `last_to_fit`, and what comes after it has no room.
        assert_eq!(
            read,
            [
                Some("x".repeat(MAX_COMPOSED)),
                None,
                Some("x".repeat(MAX_COMPOSED)),
                None
            ]
        );
    }
}
