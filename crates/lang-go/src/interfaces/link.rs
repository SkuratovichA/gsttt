//! The link of method sets: which types satisfy which interfaces, across the files of a package and across packages.
//!
//! Go never names the interface a type satisfies, so the pairs are found by comparing method sets. A type that only shares a method name with an interface is no pair: every method of the interface must be in the set of the type with the same shape.

use std::collections::{BTreeMap, BTreeSet};

use gsttt_core::connector::{LinkInput, LinkedOverride};
use gsttt_core::ids::FileId;

use super::{GoExtra, Sig};
use crate::files::NO_PACKAGE;

/// A method in a method set, with what a comparison of shapes needs to know about where it was declared.
#[derive(Clone, Copy)]
struct Member<'a> {
    sig: &'a Sig,
    /// The package of the declaring type, which an unexported name is tied to.
    package: usize,
    /// Whether the declaring type has type parameters, so the shape is compared by counts.
    generic: bool,
    /// The entity that declares the method, which a type embedding it only lends it.
    owner: usize,
}

type MethodSet<'a> = BTreeMap<&'a str, Vec<Member<'a>>>;

/// A named type or interface of one package.
struct Entity<'a> {
    package: usize,
    name: &'a str,
    interface: bool,
    /// Whether a union or a constraint term keeps the interface from being the type of a value.
    open: bool,
    generic: bool,
    /// The methods declared on it, or for an interface the methods it lists.
    own: Vec<(&'a str, &'a Sig)>,
    /// The types it embeds as the file spells them, with the file that spells them.
    embeds: Vec<(&'a str, FileId)>,
    /// The files that declare it, an embedding of it or a method of it.
    files: BTreeSet<FileId>,
}

/// Every type and interface of the project, by package and name.
struct World<'a, 'i> {
    input: &'i LinkInput<'a, GoExtra>,
    package_of: BTreeMap<FileId, usize>,
    entities: Vec<Entity<'a>>,
    lookup: BTreeMap<(usize, &'a str), usize>,
}

/// The types of the project that satisfy an interface of it, as one override for each file of the type that declares the type, an embedding in it or a method of it.
pub(crate) fn link(input: &LinkInput<'_, GoExtra>) -> Vec<LinkedOverride> {
    let world = World::build(input);
    let sets: Vec<MethodSet<'_>> = (0..world.entities.len())
        .map(|at| world.method_set(at, &mut Vec::new()))
        .collect();
    // The methods named by a value type, to test an interface only against the types that hold its rarest method.
    let mut holders: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (at, set) in sets.iter().enumerate() {
        if world.is_value_type(at) {
            for name in set.keys() {
                holders.entry(name).or_default().push(at);
            }
        }
    }
    let mut found = Vec::new();
    for (head, interface) in world.entities.iter().enumerate() {
        let Some(required) = sets.get(head).filter(|set| !set.is_empty()) else {
            continue;
        };
        let Some(owner) = interface.files.first().copied() else {
            continue;
        };
        if !interface.interface || !world.closed(head, &mut Vec::new()) {
            continue;
        }
        let rarest = required
            .keys()
            .min_by_key(|name| holders.get(*name).map_or(0, Vec::len));
        let Some(candidates) = rarest.and_then(|name| holders.get(name)) else {
            continue;
        };
        for &at in candidates {
            let Some(have) = sets.get(at) else {
                continue;
            };
            if at == head || !satisfies(required, have) {
                continue;
            }
            // A call through the interface may run a method that a type lends to the satisfying one, and Go has no other inheritance.
            let lenders = required
                .keys()
                .filter_map(|name| have.get(name))
                .flatten()
                .map(|member| member.owner)
                .filter(|&owner| {
                    world
                        .entities
                        .get(owner)
                        .is_some_and(|lender| !lender.interface)
                });
            for taker in std::iter::once(at).chain(lenders) {
                let Some(taker) = world.entities.get(taker) else {
                    continue;
                };
                for &file in &taker.files {
                    found.push(LinkedOverride {
                        file,
                        name: taker.name.to_owned(),
                        owner,
                        head: interface.name.to_owned(),
                    });
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Whether `have` holds every method of `required` in the same shape.
fn satisfies(required: &MethodSet<'_>, have: &MethodSet<'_>) -> bool {
    required.iter().all(|(name, wants)| {
        have.get(name).is_some_and(|offers| {
            wants
                .iter()
                .all(|want| offers.iter().any(|offer| same_method(name, want, offer)))
        })
    })
}

fn same_method(name: &str, want: &Member<'_>, offer: &Member<'_>) -> bool {
    let exported = name.chars().next().is_some_and(char::is_uppercase);
    if !exported && want.package != offer.package {
        return false;
    }
    if want.generic || offer.generic {
        // A type parameter may stand for any type, so only the counts can differ.
        return want.sig.params.len() == offer.sig.params.len()
            && want.sig.results.len() == offer.sig.results.len();
    }
    want.sig == offer.sig
}

impl<'a, 'i> World<'a, 'i> {
    fn build(input: &'i LinkInput<'a, GoExtra>) -> Self {
        let mut packages: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        let mut package_of = BTreeMap::new();
        for file in input.files() {
            let Some(clause) = file.facts.package.as_deref() else {
                continue;
            };
            if clause == NO_PACKAGE {
                continue;
            }
            let next = packages.len();
            let package = *packages.entry((file.path.parent(), clause)).or_insert(next);
            package_of.insert(file.id, package);
        }
        let mut world = Self {
            input,
            package_of,
            entities: Vec::new(),
            lookup: BTreeMap::new(),
        };
        for file in input.files() {
            let Some(&package) = world.package_of.get(&file.id) else {
                continue;
            };
            let extra: &'a GoExtra = file.extra;
            for fact in &extra.methods {
                let at = world.slot(package, &fact.receiver);
                if let Some(entity) = world.entities.get_mut(at) {
                    entity.own.push((&fact.method.name, &fact.method.sig));
                    entity.files.insert(file.id);
                }
            }
            for fact in &extra.interfaces {
                let at = world.slot(package, &fact.name);
                if let Some(entity) = world.entities.get_mut(at) {
                    entity.interface = true;
                    entity.open |= fact.open;
                    entity
                        .own
                        .extend(fact.methods.iter().map(|m| (m.name.as_str(), &m.sig)));
                    entity.embeds.extend(
                        fact.embeds
                            .iter()
                            .map(|spelled| (spelled.as_str(), file.id)),
                    );
                    entity.files.insert(file.id);
                }
            }
            for (name, spelled) in &extra.embeds {
                let at = world.slot(package, name);
                if let Some(entity) = world.entities.get_mut(at) {
                    entity.embeds.push((spelled, file.id));
                    entity.files.insert(file.id);
                }
            }
            for name in &extra.generic {
                let at = world.slot(package, name);
                if let Some(entity) = world.entities.get_mut(at) {
                    entity.generic = true;
                }
            }
        }
        world
    }

    /// The index of the entity `name` of `package`, made when it is new.
    fn slot(&mut self, package: usize, name: &'a str) -> usize {
        if let Some(&at) = self.lookup.get(&(package, name)) {
            return at;
        }
        let at = self.entities.len();
        self.entities.push(Entity {
            package,
            name,
            interface: false,
            open: false,
            generic: false,
            own: Vec::new(),
            embeds: Vec::new(),
            files: BTreeSet::new(),
        });
        self.lookup.insert((package, name), at);
        at
    }

    /// The entity an embedding of `spelled`, written in `file` inside `package`, leads to; none when it leads outside the project.
    fn target(&self, package: usize, spelled: &str, file: FileId) -> Option<usize> {
        let Some((_, name)) = spelled.split_once('.') else {
            return self.lookup.get(&(package, spelled)).copied();
        };
        let from = self.input.get(file)?;
        let import = from
            .imports
            .iter()
            .find(|resolved| resolved.import.exported.iter().any(|e| e == spelled))?;
        import.to.files().iter().find_map(|reached| {
            let &other = self.package_of.get(reached)?;
            self.lookup.get(&(other, name)).copied()
        })
    }

    /// The methods of an entity: its own, then those of what it embeds, an own method hiding a promoted one of its name.
    fn method_set(&self, at: usize, path: &mut Vec<usize>) -> MethodSet<'a> {
        let mut set = MethodSet::new();
        let Some(entity) = self.entities.get(at) else {
            return set;
        };
        if path.contains(&at) {
            return set;
        }
        path.push(at);
        for &(name, sig) in &entity.own {
            set.entry(name).or_default().push(Member {
                sig,
                package: entity.package,
                generic: entity.generic,
                owner: at,
            });
        }
        for &(spelled, file) in &entity.embeds {
            let Some(target) = self.target(entity.package, spelled, file) else {
                continue;
            };
            for (name, members) in self.method_set(target, path) {
                if !entity.own.iter().any(|(own, _)| *own == name) {
                    set.entry(name).or_default().extend(members);
                }
            }
        }
        path.pop();
        set
    }

    /// Whether the entity is an interface with no union or constraint term in it or in what it embeds.
    fn closed(&self, at: usize, path: &mut Vec<usize>) -> bool {
        let Some(entity) = self.entities.get(at) else {
            return false;
        };
        if !entity.interface || entity.open {
            return false;
        }
        if path.contains(&at) {
            return true;
        }
        path.push(at);
        let closed = entity.embeds.iter().all(|&(spelled, file)| {
            self.target(entity.package, spelled, file)
                .is_none_or(|target| self.closed(target, path))
        });
        path.pop();
        closed
    }

    /// Whether a value can have the entity as its type: a type, or an interface that is no constraint.
    fn is_value_type(&self, at: usize) -> bool {
        self.entities
            .get(at)
            .is_some_and(|entity| !entity.interface || self.closed(at, &mut Vec::new()))
    }
}
