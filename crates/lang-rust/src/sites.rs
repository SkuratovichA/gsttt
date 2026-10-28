//! Contract sites: the calls, method calls, macro calls and attributes of a file, matched against the rules of the
//! packs in force, and the member chains and assignments that the other two rule kinds read.
//!
//! [`candidate`] says how a site is named and how its arguments are read, [`receiver`] how a rule that wants a
//! particular receiver finds out whether it has one, [`chain`] the two rule kinds that read names, not calls, and
//! [`routers`] which routes are served together and which routers are mounted in which.

mod candidate;
mod chain;
mod receiver;
mod routers;
#[cfg(test)]
mod tests;

use gsttt_core::fact::{ContractFact, Embedded, UnresolvedSite};
use gsttt_core::rules::{AssignRule, CallRule, ChainRule, DecoratorRule, EmbedRule, Outcome, Pack};
use gsttt_core::text::LineIndex;
use ra_ap_edition::Edition;
use ra_ap_syntax::{SyntaxKind, SyntaxNode};
use std::cell::OnceCell;

use self::candidate::{Candidate, Prepared, View, apply_per_verb, lacks_keywords};
use crate::link::Extra;

/// What the sites of a file say.
#[derive(Default)]
pub(crate) struct Sites {
    pub(crate) found: Vec<Found>,
    pub(crate) embedded: Vec<Embedded>,
    pub(crate) unresolved: Vec<UnresolvedSite>,
    /// What linking needs, read from the routers of the file.
    pub(crate) extra: Extra,
}

/// A contract site with the place it came from, which the link step uses to tell which routes belong together.
pub(crate) struct Found {
    pub(crate) fact: ContractFact,
    /// Where the router chain or the item the site belongs to starts in the file.
    pub(crate) anchor: u32,
}

impl Sites {
    /// Sorts and drops repeats, so the same file always gives the same facts in the same order.
    fn finish(&mut self) {
        self.found.sort_by(|a, b| {
            (a.fact.line, &a.fact.key, &a.fact.via, a.anchor).cmp(&(
                b.fact.line,
                &b.fact.key,
                &b.fact.via,
                b.anchor,
            ))
        });
        self.found
            .dedup_by(|a, b| a.fact == b.fact && a.anchor == b.anchor);
        self.embedded
            .sort_by(|a, b| (a.line, &a.lang, &a.text).cmp(&(b.line, &b.lang, &b.text)));
        self.embedded.dedup();
        self.unresolved
            .sort_by(|a, b| (a.line, &a.detail).cmp(&(b.line, &b.detail)));
        self.unresolved.dedup();
    }
}

/// Matches the sites of a file against `packs`.
pub(crate) fn read(
    root: &SyntaxNode,
    lines: &LineIndex,
    packs: &[Pack],
    edition: Edition,
) -> Sites {
    let reader = Reader::new(root, lines, packs, edition);
    let mut sites = Sites::default();
    if reader.is_idle() {
        return sites;
    }
    for node in root.descendants() {
        reader.visit(&node, &mut sites);
    }
    // Before `finish`, so the sites that routers add are sorted and deduplicated with the rest.
    reader.routers(root, &mut sites);
    sites.finish();
    sites
}

/// The rules of the packs in force, by kind, with the pack each belongs to.
struct Reader<'a> {
    root: &'a SyntaxNode,
    lines: &'a LineIndex,
    calls: Vec<(&'a Pack, &'a CallRule)>,
    decorators: Vec<(&'a Pack, &'a DecoratorRule)>,
    embeds: Vec<(&'a Pack, &'a EmbedRule)>,
    chains: Vec<(&'a Pack, &'a ChainRule)>,
    assigns: Vec<(&'a Pack, &'a AssignRule)>,
    /// Whether the axum pack is in force, which is what makes `nest` and `merge` mount a router.
    axum: bool,
    /// Whether the actix pack is in force, which is what makes `scope` and `service` mount one.
    actix: bool,
    /// The edition of the file, which the arguments of macros and attributes are parsed in.
    edition: Edition,
    /// What the receivers of the file can be traced to, found when a rule with a receiver first asks.
    traced: OnceCell<receiver::Scope>,
}

/// What the rules produced at one site. A site that yields a contract is not also a blind spot.
#[derive(Default)]
struct Outcomes {
    facts: Vec<ContractFact>,
    blind: Vec<UnresolvedSite>,
}

impl Outcomes {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Fact(fact) => self.facts.push(fact),
            Outcome::Blind(site) => self.blind.push(site),
            Outcome::Skip => {}
        }
    }

    fn flush(self, anchor: u32, sites: &mut Sites) {
        if self.facts.is_empty() {
            // However many rules read it, a site is one blind spot.
            sites.unresolved.extend(self.blind.into_iter().take(1));
        } else {
            sites
                .found
                .extend(self.facts.into_iter().map(|fact| Found { fact, anchor }));
        }
    }
}

impl<'a> Reader<'a> {
    fn new(
        root: &'a SyntaxNode,
        lines: &'a LineIndex,
        packs: &'a [Pack],
        edition: Edition,
    ) -> Self {
        Self {
            root,
            lines,
            calls: packs
                .iter()
                .flat_map(|pack| pack.calls.iter().map(move |rule| (pack, rule)))
                .collect(),
            // A class or a receiver is where a decorator in another language finds its prefix, and an attribute has
            // neither.
            decorators: packs
                .iter()
                .flat_map(|pack| pack.decorators.iter().map(move |rule| (pack, rule)))
                .filter(|(_, rule)| rule.class.is_none() && rule.receiver.is_none())
                .collect(),
            embeds: packs
                .iter()
                .flat_map(|pack| pack.embeds.iter().map(move |rule| (pack, rule)))
                .collect(),
            chains: packs
                .iter()
                .flat_map(|pack| pack.chains.iter().map(move |rule| (pack, rule)))
                .collect(),
            assigns: packs
                .iter()
                .flat_map(|pack| pack.assigns.iter().map(move |rule| (pack, rule)))
                .collect(),
            axum: packs.iter().any(|pack| pack.name == "axum"),
            actix: packs.iter().any(|pack| pack.name == "actix"),
            edition,
            traced: OnceCell::new(),
        }
    }

    /// Whether no rule of any kind is in force, so there is nothing to look for.
    fn is_idle(&self) -> bool {
        self.calls.is_empty()
            && self.decorators.is_empty()
            && self.embeds.is_empty()
            && self.chains.is_empty()
            && self.assigns.is_empty()
    }

    fn visit(&self, node: &SyntaxNode, sites: &mut Sites) {
        match node.kind() {
            SyntaxKind::CALL_EXPR | SyntaxKind::MACRO_CALL => self.call(node, sites),
            SyntaxKind::METHOD_CALL_EXPR => {
                self.call(node, sites);
                self.chain(node, sites);
            }
            SyntaxKind::PATH_EXPR | SyntaxKind::FIELD_EXPR | SyntaxKind::INDEX_EXPR => {
                self.chain(node, sites);
            }
            SyntaxKind::ATTR => self.attribute(node, sites),
            SyntaxKind::CONST
            | SyntaxKind::STATIC
            | SyntaxKind::LET_STMT
            | SyntaxKind::BIN_EXPR
            | SyntaxKind::RECORD_EXPR_FIELD => self.assign(node, sites),
            _ => {}
        }
    }

    fn call(&self, node: &SyntaxNode, sites: &mut Sites) {
        if self.calls.is_empty() && self.embeds.is_empty() {
            return;
        }
        let Some(candidate) = Candidate::of_call(node, self.lines) else {
            return;
        };
        let calls: Vec<(&Pack, &CallRule)> = self
            .calls
            .iter()
            .filter(|(_, rule)| rule.callee.matches(&candidate.callee))
            .copied()
            .collect();
        let embeds: Vec<(&Pack, &EmbedRule)> = self
            .embeds
            .iter()
            .filter(|(_, rule)| rule.callee.matches(&candidate.callee))
            .copied()
            .collect();
        if calls.is_empty() && embeds.is_empty() {
            return;
        }
        let prepared = candidate.prepare(self.edition);
        let mut outcomes = Outcomes::default();
        for (pack, rule) in calls {
            self.apply_call(pack, rule, &candidate, &prepared, &mut outcomes);
        }
        outcomes.flush(candidate.anchor, sites);
        let view = View::new(&prepared);
        for (pack, rule) in embeds {
            match rule.apply(&pack.name, &view, candidate.line, &candidate.callee) {
                Ok(document) => sites.embedded.push(document),
                Err(blind) => sites.unresolved.push(blind),
            }
        }
    }

    fn apply_call(
        &self,
        pack: &Pack,
        rule: &CallRule,
        candidate: &Candidate,
        prepared: &Prepared,
        outcomes: &mut Outcomes,
    ) {
        let prefix = match &rule.receiver {
            // A receiver the file does not show to be what the rule wants is not a site at all.
            Some(receiver) => match self.gate(candidate, receiver) {
                // A prefix that is empty is no prefix, so a request still has to name a URL to count.
                Some(prefix) => Some(prefix).filter(|prefix| !prefix.is_empty()),
                None => return,
            },
            None => None,
        };
        let per_verb = apply_per_verb(rule, prepared, |view| {
            rule.apply(
                &pack.name,
                view,
                prefix.as_deref(),
                candidate.line,
                &candidate.callee,
            )
        });
        for outcome in per_verb {
            outcomes.add(outcome);
        }
    }

    fn attribute(&self, node: &SyntaxNode, sites: &mut Sites) {
        if self.decorators.is_empty() {
            return;
        }
        let Some(candidate) = Candidate::of_attribute(node, self.lines) else {
            return;
        };
        let rules: Vec<(&Pack, &DecoratorRule)> = self
            .decorators
            .iter()
            .filter(|(_, rule)| rule.decorator.matches(&candidate.callee))
            .copied()
            .collect();
        if rules.is_empty() {
            return;
        }
        let prepared = candidate.prepare(self.edition);
        let mut outcomes = Outcomes::default();
        for (pack, rule) in rules {
            for variant in prepared.per_method(rule.method.as_ref()) {
                if lacks_keywords(&rule.key, &variant) {
                    continue;
                }
                let view = View::new(&variant);
                outcomes.add(rule.apply(
                    &pack.name,
                    &view,
                    None,
                    candidate.line,
                    &candidate.callee,
                ));
            }
        }
        outcomes.flush(candidate.anchor, sites);
    }
}
