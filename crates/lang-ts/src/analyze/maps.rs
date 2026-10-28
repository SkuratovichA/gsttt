//! Contracts that are read from the shape of an object literal rather than from a call: GraphQL resolver maps and tRPC routers.

use gsttt_core::contract::{ContractKind, Role};
use gsttt_core::fact::ContractFact;
use gsttt_core::rules::contract_key;
use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectExpression, ObjectPropertyKind};

use super::Analyzer;
use crate::extra::{TrpcEntry, TrpcRouter};
use crate::shape::{chain_of, dotted, property_name};

/// The operation types a resolver map names its fields under.
const ROOTS: [&str; 3] = ["Query", "Mutation", "Subscription"];

impl Analyzer<'_> {
    /// `{ Query: { bookings() {} }, Mutation: { cancelBooking() {} } }` serves `graphql Query.bookings` and `graphql Mutation.cancelBooking`; types other than the roots are not entry points.
    pub(super) fn resolver_map(&mut self, object: &ObjectExpression<'_>) {
        for root in &object.properties {
            let ObjectPropertyKind::ObjectProperty(root) = root else {
                continue;
            };
            if root.computed {
                continue;
            }
            let Some(root_name) = property_name(&root.key) else {
                continue;
            };
            if !ROOTS.contains(&root_name.as_str()) {
                continue;
            }
            let Expression::ObjectExpression(fields) = root.value.get_inner_expression() else {
                continue;
            };
            for field in &fields.properties {
                let ObjectPropertyKind::ObjectProperty(field) = field else {
                    continue;
                };
                if field.computed {
                    continue;
                }
                let Some(name) = property_name(&field.key) else {
                    continue;
                };
                self.out.contracts.push(ContractFact {
                    key: contract_key(&ContractKind::Graphql, Some(&root_name), &name, None),
                    role: Role::Serves,
                    line: self.line(field.span.start),
                    via: "graphql".to_owned(),
                });
            }
        }
    }

    /// `router({ get: procedure.query(…), nested: router({…}), other })` declares a tRPC router. The link phase puts the routers together, because a router is mostly composed in another file than it is built in.
    pub(super) fn trpc_router(&mut self, call: &CallExpression<'_>, callee: &str) {
        if !self.has_trpc || !is_router_callee(callee) {
            return;
        }
        let Some(Argument::ObjectExpression(object)) = call.arguments.first() else {
            return;
        };
        let line = self.line(call.span.start);
        let id = self
            .names
            .get(&call.span.start)
            .cloned()
            .unwrap_or_else(|| format!("#{line}"));
        let mut entries = Vec::new();
        for property in &object.properties {
            let property = match property {
                ObjectPropertyKind::ObjectProperty(property) => property,
                ObjectPropertyKind::SpreadProperty(spread) => {
                    self.skipped_entry("spread entry", spread.span);
                    continue;
                }
            };
            if property.computed {
                continue;
            }
            let Some(name) = property_name(&property.key) else {
                continue;
            };
            match property.value.get_inner_expression() {
                Expression::Identifier(local) => entries.push(TrpcEntry::Ref {
                    name,
                    local: local.name.as_str().to_owned(),
                    line: self.line(property.span.start),
                }),
                Expression::CallExpression(inner) if is_procedure(inner) => {
                    entries.push(TrpcEntry::Procedure {
                        name,
                        line: self.line(property.span.start),
                    });
                }
                Expression::CallExpression(inner)
                    if chain_of(&inner.callee)
                        .and_then(|names| dotted(&names))
                        .is_some_and(|inner_callee| is_router_callee(&inner_callee)) =>
                {
                    let nested = format!("{id}.{name}");
                    self.names.insert(inner.span.start, nested.clone());
                    entries.push(TrpcEntry::Nested { name, id: nested });
                }
                Expression::StaticMemberExpression(_)
                | Expression::ComputedMemberExpression(_)
                | Expression::PrivateFieldExpression(_) => {
                    self.skipped_entry("member entry", property.span);
                }
                _ => {}
            }
        }
        if !entries.is_empty() {
            self.out.extra.routers.push(TrpcRouter { id, entries });
        }
    }

    /// A router entry the link phase cannot follow becomes a site this file could not read.
    fn skipped_entry(&mut self, form: &str, span: oxc_span::Span) {
        let site = gsttt_core::fact::UnresolvedSite {
            line: self.line(span.start),
            reason: gsttt_core::fact::UnresolvedReason::Unsupported(format!("tRPC router {form}")),
            detail: self.snippet(span),
        };
        self.out.unresolved.push(site);
    }
}

fn is_router_callee(callee: &str) -> bool {
    matches!(
        callee.rsplit('.').next(),
        Some("router" | "createTRPCRouter")
    )
}

/// `procedure.input(schema).query(handler)`: the call ends in `.query`, `.mutation` or `.subscription`.
pub(super) fn is_procedure(call: &CallExpression<'_>) -> bool {
    matches!(
        &call.callee,
        Expression::StaticMemberExpression(member)
            if matches!(member.property.name.as_str(), "query" | "mutation" | "subscription")
    )
}
