//! The names a block binds, in the order the source binds them.

use crate::routers::Router;

/// A type a value holds, as far as its written type tells: the head of the type, with the import it is qualified by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Head {
    /// The index of the import that qualifies the type, `None` for a type of the package.
    pub(crate) import: Option<usize>,
    /// The name of the type.
    pub(crate) name: String,
}

/// One name a scope binds.
#[derive(Clone, Debug)]
pub(crate) struct Binding<'a> {
    pub(crate) name: &'a str,
    /// The type the name holds, when what it was bound to tells.
    pub(crate) holds: Option<Head>,
    /// The text the name stands for, when it is bound to a constant string.
    pub(crate) text: Option<String>,
    /// The router the name holds, when it was made by a call that makes one or by a group of another router.
    pub(crate) router: Option<Router>,
}

/// The stack of scopes of the code being walked. Bindings are pushed as the source reaches them, so a name that is declared after its use is not yet bound at the use.
#[derive(Debug, Default)]
pub(crate) struct Scopes<'a> {
    bindings: Vec<Binding<'a>>,
    marks: Vec<usize>,
}

impl<'a> Scopes<'a> {
    pub(crate) fn push_scope(&mut self) {
        self.marks.push(self.bindings.len());
    }

    pub(crate) fn pop_scope(&mut self) {
        if let Some(mark) = self.marks.pop() {
            self.bindings.truncate(mark);
        }
    }

    /// Binds `name` in the innermost scope. The blank identifier binds nothing.
    pub(crate) fn bind(&mut self, name: &'a str, holds: Option<Head>, text: Option<String>) {
        if name != "_" {
            self.bindings.push(Binding {
                name,
                holds,
                text,
                router: None,
            });
        }
    }

    /// Notes that the innermost binding of `name` holds `router`.
    pub(crate) fn set_router(&mut self, name: &str, router: Router) {
        if let Some(binding) = self
            .bindings
            .iter_mut()
            .rev()
            .find(|binding| binding.name == name)
        {
            binding.router = Some(router);
        }
    }

    /// The innermost binding of `name`.
    pub(crate) fn find(&self, name: &str) -> Option<&Binding<'a>> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| binding.name == name)
    }
}
