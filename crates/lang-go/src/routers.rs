//! Routers: what a local router carries from the call that made it, the groups and closures that add a prefix to it, and the gate that keeps `cache.Get("/k", v)` from being a route.

use gsttt_core::contract::ContractKind;
use gsttt_core::rules::{CallRule, Reading, join};
use tree_sitter::Node;

use crate::scope::Head;
use crate::syntax::{field_text, fields, named_children, text_of};
use crate::value::Reader;
use crate::walk::Walk;

/// A router a local holds: the call that made it, the type it was reached through, and the path every route on it sits behind.
#[derive(Clone, Debug)]
pub(crate) struct Router {
    /// The callee that made the router, as the rules see it: `chi.NewRouter`.
    pub(crate) made_by: Option<String>,
    /// The spelled type of the router a group was taken from, so that a group of a typed router is typed too.
    pub(crate) held: Option<String>,
    /// The path of the groups the router sits in: absent for a router made on its own, unread when a group's prefix is no text.
    pub(crate) prefix: Reading,
}

/// What is known of the receiver of a call, `r` in `r.Get("/x", h)`.
#[derive(Debug, Default)]
pub(crate) struct Receiver {
    /// The type the receiver holds, spelled by the name its package is known by: `chi.Router`.
    pub(crate) held: Option<String>,
    pub(crate) router: Option<Router>,
}

impl Receiver {
    /// Whether a rule that asks for a receiver reads a call on this one: its type is listed, or the local was made by a call the rule names.
    pub(crate) fn passes(&self, rule: &CallRule) -> bool {
        let held = self.held.as_deref().into_iter().chain(
            self.router
                .as_ref()
                .and_then(|router| router.held.as_deref()),
        );
        let typed = held
            .into_iter()
            .any(|held| rule.receiver_types.iter().any(|ty| ty == held));
        typed
            || self
                .router
                .as_ref()
                .and_then(|router| router.made_by.as_deref())
                .zip(rule.receiver.as_ref())
                .is_some_and(|(made_by, receiver)| receiver.from.matches(made_by))
    }

    /// The path the routes on this receiver sit behind, when it is a router that carries one.
    pub(crate) fn prefix(&self) -> Option<&Reading> {
        self.router.as_ref().map(|router| &router.prefix)
    }
}

/// How the rules spell a type: the package by the name it is known by, as a callee is.
fn spelled(cx: &crate::file::FileCx<'_>, head: &Head) -> String {
    match head.import.and_then(|index| cx.imports.get(index)) {
        Some(import) => format!("{}.{}", import.callee_name, head.name),
        None => head.name.clone(),
    }
}

/// The path of a group `text` inside a router that sits behind `base`; `None` is a prefix that is no text.
fn extend(base: &Reading, text: Option<String>) -> Reading {
    match (base, text) {
        (Reading::Unread, _) | (_, None) => Reading::Unread,
        (Reading::Text(base), Some(text)) => {
            Reading::Text(join(&ContractKind::Http, base, &text).into_owned())
        }
        (Reading::Absent, Some(text)) => Reading::Text(text),
    }
}

impl<'a> Walk<'_, 'a> {
    /// Whether a pack of one of these names is in force for the project.
    fn in_force(&self, names: &[&str]) -> bool {
        self.cx
            .packs
            .iter()
            .any(|pack| names.contains(&pack.name.as_str()))
    }

    fn text_of_node(&self, node: Node<'a>) -> Option<String> {
        Reader {
            cx: self.cx,
            scopes: &self.scopes,
        }
        .text(node)
    }

    /// The operand `With(mw)` is called on, for `r.With(mw).Get(...)`: the middleware wraps the same router, so the callee reads `r.Get`.
    pub(crate) fn through_with(&self, call: Node<'a>) -> Option<Node<'a>> {
        let function = call.child_by_field_name("function")?;
        (function.kind() == "selector_expression"
            && field_text(function, "field", self.cx.source) == Some("With")
            && self.in_force(&["chi"]))
        .then(|| function.child_by_field_name("operand"))?
    }

    /// The operand of the selector a call is made on, with the calls of `With` taken away.
    pub(crate) fn receiver_operand(&self, function: Node<'a>) -> Option<Node<'a>> {
        if function.kind() != "selector_expression" {
            return None;
        }
        let mut operand = function.child_by_field_name("operand")?;
        while operand.kind() == "call_expression" {
            operand = self.through_with(operand)?;
        }
        Some(operand)
    }

    /// What the file shows of the receiver: a local through its binding, a field of a struct of the file through the type it is written as.
    pub(crate) fn receiver_of(&self, operand: Node<'a>) -> Receiver {
        let source = self.cx.source;
        match operand.kind() {
            "identifier" => {
                let name = text_of(operand, source);
                match self.scopes.find(name) {
                    Some(binding) => Receiver {
                        held: binding.holds.as_ref().map(|head| spelled(self.cx, head)),
                        router: binding.router.clone(),
                    },
                    // A variable of the package, such as `var app *fiber.App` that `init` assigns.
                    None => Receiver {
                        held: self.cx.vars.get(name).map(|head| spelled(self.cx, head)),
                        router: None,
                    },
                }
            }
            "selector_expression" => self.field_receiver(operand).unwrap_or_default(),
            _ => Receiver::default(),
        }
    }

    /// `s.router` where `s` holds a struct of this file and `router` is written there as a type.
    fn field_receiver(&self, selector: Node<'a>) -> Option<Receiver> {
        let source = self.cx.source;
        let holder = selector.child_by_field_name("operand")?;
        if holder.kind() != "identifier" {
            return None;
        }
        let holder = self.scopes.find(text_of(holder, source))?.holds.as_ref()?;
        if holder.import.is_some() {
            return None;
        }
        let head = self
            .cx
            .structs
            .get(&holder.name)?
            .get(field_text(selector, "field", source)?)?
            .as_ref()?;
        Some(Receiver {
            held: Some(spelled(self.cx, head)),
            router: None,
        })
    }

    /// Whether some rule of a pack in force takes a receiver made by `callee`.
    fn makes_router(&self, callee: &str) -> bool {
        self.cx
            .packs
            .iter()
            .flat_map(|pack| &pack.calls)
            .filter_map(|rule| rule.receiver.as_ref())
            .any(|receiver| receiver.from.matches(callee))
    }

    /// The router a value makes: a call a rule names as the maker of a receiver, `chi.NewRouter()`, or a group of a router, `r.Group("/v1")`, `r.PathPrefix("/api").Subrouter()`.
    pub(crate) fn router_of(&self, value: Node<'a>) -> Option<Router> {
        if value.kind() != "call_expression" {
            return None;
        }
        let function = value.child_by_field_name("function")?;
        if let Some(callee) = self.callee_of(function)
            && !callee.local
            && self.makes_router(&callee.written)
        {
            return Some(Router {
                made_by: Some(callee.written),
                held: None,
                prefix: Reading::Absent,
            });
        }
        if function.kind() != "selector_expression" {
            return None;
        }
        let arguments = named_children(value.child_by_field_name("arguments")?);
        match field_text(function, "field", self.cx.source)? {
            // `app.Group("/x").Use(mw)` is the group, with the middleware on it.
            "Use" if self.in_force(&["gin", "echo", "fiber"]) => {
                self.router_of(function.child_by_field_name("operand")?)
            }
            "Group" if self.in_force(&["gin", "echo", "fiber"]) => {
                let prefix = arguments
                    .first()
                    .filter(|first| first.kind() != "func_literal")?;
                let parent = self.receiver_of(self.receiver_operand(function)?);
                Some(Self::group_of(parent, self.text_of_node(*prefix)))
            }
            "Subrouter" if self.in_force(&["gorilla"]) => {
                let path_prefix = function.child_by_field_name("operand")?;
                let inner = path_prefix.child_by_field_name("function")?;
                if path_prefix.kind() != "call_expression"
                    || field_text(inner, "field", self.cx.source) != Some("PathPrefix")
                {
                    return None;
                }
                let first =
                    *named_children(path_prefix.child_by_field_name("arguments")?).first()?;
                let parent = self.receiver_of(self.receiver_operand(inner)?);
                Some(Self::group_of(parent, self.text_of_node(first)))
            }
            _ => None,
        }
    }

    /// The router a group of `parent` makes, whose path is `text` behind the path of `parent`.
    fn group_of(parent: Receiver, text: Option<String>) -> Router {
        let router = parent.router;
        let base = router
            .as_ref()
            .map_or(Reading::Absent, |router| router.prefix.clone());
        Router {
            prefix: extend(&base, text),
            held: parent
                .held
                .or_else(|| router.as_ref().and_then(|router| router.held.clone())),
            made_by: router.and_then(|router| router.made_by),
        }
    }

    /// The function literal a call hands its router to, with the router it receives: `r.Route("/x", func(r chi.Router) { … })` adds the prefix, `r.Group(func(r chi.Router) { … })` adds none.
    pub(crate) fn closure_router(
        &self,
        function: Node<'a>,
        arguments: &[Node<'a>],
    ) -> Option<(Node<'a>, Router)> {
        let (text, literal) = match field_text(function, "field", self.cx.source)? {
            "Route" if self.in_force(&["chi", "fiber"]) => match arguments {
                [path, literal, ..] => (Some(self.text_of_node(*path)), literal),
                _ => return None,
            },
            "Group" if self.in_force(&["chi"]) => match arguments {
                [literal] => (None, literal),
                _ => return None,
            },
            _ => return None,
        };
        if literal.kind() != "func_literal" {
            return None;
        }
        let parent = self.receiver_of(self.receiver_operand(function)?);
        let base = parent
            .router
            .as_ref()
            .map_or(Reading::Absent, |router| router.prefix.clone());
        let prefix = match text {
            Some(text) => extend(&base, text),
            None => base,
        };
        Some((
            *literal,
            Router {
                made_by: None,
                held: None,
                prefix,
            },
        ))
    }

    /// Notes that the first parameter of the function literal holds `router`.
    pub(crate) fn bind_closure_router(&mut self, literal: Node<'a>, router: Router) {
        let source = self.cx.source;
        let name = literal
            .child_by_field_name("parameters")
            .and_then(|list| named_children(list).into_iter().next())
            .and_then(|declaration| fields(declaration, "name").into_iter().next());
        if let Some(name) = name {
            self.scopes.set_router(text_of(name, source), router);
        }
    }

    /// The methods `.Methods("GET", "POST")` limits a gorilla route to, when `call` is the route it is called on.
    pub(crate) fn methods_of(&self, call: Node<'a>) -> Vec<String> {
        if !self.in_force(&["gorilla"]) {
            return Vec::new();
        }
        let Some(selector) = call.parent().filter(|parent| {
            parent.kind() == "selector_expression"
                && field_text(*parent, "field", self.cx.source) == Some("Methods")
                && parent.child_by_field_name("operand") == Some(call)
        }) else {
            return Vec::new();
        };
        let Some(outer) = selector
            .parent()
            .filter(|outer| outer.child_by_field_name("function") == Some(selector))
        else {
            return Vec::new();
        };
        let arguments = outer
            .child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default();
        arguments
            .into_iter()
            .filter_map(|argument| self.text_of_node(argument))
            .collect()
    }
}
