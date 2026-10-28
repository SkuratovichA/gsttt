//! What the imports of a file bind, and the dotted names of the file read through them. After `import requests as r`,
//! `r.get` is also `requests.get`, which is the spelling the packs name; after `from requests import post as send`,
//! `send` is `requests.post`.

use std::collections::HashMap;

use gsttt_core::fact::Line;
use gsttt_core::rules::OneOrMany;
use ruff_python_ast::Expr;

use crate::extra::Bind;
use crate::site::write_dotted;

/// One name an import binds.
#[derive(Debug)]
struct Bound<'a> {
    local: &'a str,
    /// The module as written, without the dots of a relative import.
    module: &'a str,
    /// How many dots the module starts with.
    level: u32,
    /// The name taken from the module in `from module import name`.
    name: Option<&'a str>,
    line: Line,
}

/// The names the imports of a file bind, in source order. The file is read as one scope, which is how a module-level
/// alias is used, so from an import on, a name stands for the last import that bound it. A name used above its import
/// is not followed.
#[derive(Debug, Default)]
pub(crate) struct Imports<'a> {
    bound: Vec<Bound<'a>>,
    /// For each local name that an import binds to another name, the index in `bound` of the last import to bind it.
    renamed: HashMap<&'a str, usize>,
}

impl<'a> Imports<'a> {
    /// `import module` or `import module as alias`.
    pub(crate) fn module(&mut self, module: &'a str, alias: Option<&'a str>, line: Line) {
        match alias {
            Some(local) => self.bind(Bound {
                local,
                module,
                level: 0,
                name: None,
                line,
            }),
            // `import a.b.c` binds `a`, which is not the module the import resolves to: no router is reached through
            // it, and the name stands for itself.
            None if module.contains('.') => {
                let first = module.split('.').next().unwrap_or(module);
                self.renamed.remove(first);
            }
            None => self.bind(Bound {
                local: module,
                module,
                level: 0,
                name: None,
                line,
            }),
        }
    }

    /// `from module import name` or `from module import name as alias`, with `level` dots in front of the module.
    pub(crate) fn name(
        &mut self,
        module: &'a str,
        level: u32,
        name: &'a str,
        alias: Option<&'a str>,
        line: Line,
    ) {
        self.bind(Bound {
            local: alias.unwrap_or(name),
            module,
            level,
            name: Some(name),
            line,
        });
    }

    fn bind(&mut self, bound: Bound<'a>) {
        // A name that stands for itself needs no rewriting, and binding it ends an earlier alias of the same name.
        if bound.level == 0 && bound.name.is_none() && bound.local == bound.module {
            self.renamed.remove(bound.local);
        } else {
            self.renamed.insert(bound.local, self.bound.len());
        }
        self.bound.push(bound);
    }

    /// Appends `name` to `out` with its first segment replaced by what an import binds that segment to, and tells
    /// whether one did. A name bound by a relative import is left as written: it names a module of the repository,
    /// which the packs do not.
    fn rewrite(&self, name: &str, out: &mut String) -> bool {
        let head_end = name.find('.').unwrap_or(name.len());
        let Some((head, tail)) = name.split_at_checked(head_end) else {
            return false;
        };
        let Some(bound) = self
            .renamed
            .get(head)
            .and_then(|&index| self.bound.get(index))
            .filter(|bound| bound.level == 0)
        else {
            return false;
        };
        out.push_str(bound.module);
        if let Some(name) = bound.name {
            out.push('.');
            out.push_str(name);
        }
        out.push_str(tail);
        true
    }

    /// The names the imports bind, which `link` follows a mounted router through.
    pub(crate) fn binds(&self) -> Vec<Bind> {
        self.bound
            .iter()
            .map(|bound| Bind {
                local: bound.local.to_owned(),
                spec: specifier(bound.level, bound.module),
                line: bound.line,
                name: bound.name.map(str::to_owned),
            })
            .collect()
    }
}

/// The specifier of an import as the resolver reads it: the module behind as many dots as its level.
pub(crate) fn specifier(level: u32, module: &str) -> String {
    let dots = usize::try_from(level).unwrap_or_default();
    format!("{}{module}", ".".repeat(dots))
}

/// A dotted name as a file writes it and, when an import binds its first segment to another name, as the import
/// spells it: `r.get` is also `requests.get` after `import requests as r`. A rule applies when its pattern matches
/// either, so a pack written for the local spelling, `op.create_table`, keeps working next to one written for the
/// library's, `alembic.op.create_table`.
///
/// It owns its text so that a name read for each call of a file can reuse one buffer.
#[derive(Debug, Default)]
pub(crate) struct Name {
    written: String,
    imported: String,
    /// Whether `imported` holds a spelling that differs from `written`.
    renamed: bool,
}

impl Name {
    /// The name `expr` spells, or `None` when it is not a name or a chain of attributes.
    pub(crate) fn of(expr: &Expr, imports: &Imports<'_>) -> Option<Self> {
        let mut name = Self::default();
        name.read(expr, imports).then_some(name)
    }

    /// Replaces the name with the one `expr` spells, keeping the buffers, and tells whether `expr` is a name.
    pub(crate) fn read(&mut self, expr: &Expr, imports: &Imports<'_>) -> bool {
        self.written.clear();
        self.imported.clear();
        self.renamed = false;
        if !write_dotted(expr, &mut self.written) {
            return false;
        }
        self.renamed = imports.rewrite(&self.written, &mut self.imported);
        true
    }

    /// The name as the file writes it.
    pub(crate) fn written(&self) -> &str {
        &self.written
    }

    /// The name as the imports spell it, which is the name as written when none of them binds its first segment.
    pub(crate) fn imported(&self) -> &str {
        if self.renamed {
            &self.imported
        } else {
            &self.written
        }
    }

    /// The spelling that `patterns` match, the one as written first.
    pub(crate) fn matching(&self, patterns: &OneOrMany) -> Option<&str> {
        if patterns.matches(&self.written) {
            Some(&self.written)
        } else if self.renamed && patterns.matches(&self.imported) {
            Some(&self.imported)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn rewritten(imports: &Imports<'_>, name: &str) -> Option<String> {
        let mut out = String::new();
        imports.rewrite(name, &mut out).then_some(out)
    }

    #[test]
    fn a_module_alias_replaces_the_first_segment() {
        let mut imports = Imports::default();
        imports.module("requests", Some("r"), 1);
        assert_eq!(
            rewritten(&imports, "r.get").as_deref(),
            Some("requests.get")
        );
        assert_eq!(rewritten(&imports, "r").as_deref(), Some("requests"));
        assert_eq!(rewritten(&imports, "requests.get"), None);
    }

    #[test]
    fn a_name_taken_from_a_module_is_its_full_name() {
        let mut imports = Imports::default();
        imports.name("requests", 0, "get", None, 1);
        imports.name("sqlalchemy", 0, "text", Some("t"), 2);
        assert_eq!(rewritten(&imports, "get").as_deref(), Some("requests.get"));
        assert_eq!(rewritten(&imports, "t").as_deref(), Some("sqlalchemy.text"));
        assert_eq!(rewritten(&imports, "text"), None);
    }

    #[test]
    fn a_dotted_module_under_an_alias_keeps_its_path() {
        let mut imports = Imports::default();
        imports.module("xml.etree.ElementTree", Some("ET"), 1);
        assert_eq!(
            rewritten(&imports, "ET.parse").as_deref(),
            Some("xml.etree.ElementTree.parse")
        );
    }

    #[test]
    fn a_relative_import_is_left_as_written() {
        let mut imports = Imports::default();
        imports.name("", 1, "views", None, 1);
        imports.name("compat", 2, "text", Some("t"), 2);
        assert_eq!(rewritten(&imports, "views.detail"), None);
        assert_eq!(rewritten(&imports, "t"), None);
    }

    #[test]
    fn the_last_import_of_a_name_wins() {
        let mut imports = Imports::default();
        imports.module("mylib", Some("requests"), 1);
        assert_eq!(
            rewritten(&imports, "requests.get").as_deref(),
            Some("mylib.get")
        );
        // Binding `requests` as itself ends the alias, and so does `import requests.adapters`.
        imports.module("requests", None, 2);
        assert_eq!(rewritten(&imports, "requests.get"), None);
        imports.module("mylib", Some("requests"), 3);
        imports.module("requests.adapters", None, 4);
        assert_eq!(rewritten(&imports, "requests.get"), None);
        // A relative import after an absolute one ends the rewriting too.
        imports.module("mylib", Some("requests"), 5);
        imports.name("", 1, "requests", None, 6);
        assert_eq!(rewritten(&imports, "requests.get"), None);
    }

    #[test]
    fn a_name_nothing_binds_is_not_rewritten() {
        let mut imports = Imports::default();
        imports.module("os", Some("_os"), 1);
        for name in ["", "?", "?.get", "self.router.get", "os.environ"] {
            assert_eq!(rewritten(&imports, name), None, "{name:?}");
        }
    }

    #[test]
    fn binds_keep_the_specifier_the_resolver_reads() {
        let mut imports = Imports::default();
        imports.module("bookings", None, 1);
        imports.module("a.b.c", None, 2);
        imports.module("app.api", Some("api"), 3);
        imports.name("", 1, "views", None, 4);
        imports.name("app", 0, "router", Some("r"), 5);
        let shown: Vec<String> = imports
            .binds()
            .iter()
            .map(|bind| {
                format!(
                    "{}={}{}@{}",
                    bind.local,
                    bind.spec,
                    bind.name
                        .as_deref()
                        .map_or_else(String::new, |name| format!(":{name}")),
                    bind.line
                )
            })
            .collect();
        // `import a.b.c` binds `a`, which is not the module it resolves to, so it is not a bind.
        assert_eq!(
            shown,
            [
                "bookings=bookings@1",
                "api=app.api@3",
                "views=.:views@4",
                "r=app:router@5"
            ]
        );
    }

    proptest! {
        #[test]
        fn an_alias_stands_for_its_full_name_in_any_dotted_name(
            local in "[a-z_][a-z0-9_]{0,8}",
            module in "[a-z][a-z0-9_]{0,8}(\\.[a-z][a-z0-9_]{0,8}){0,2}",
            name in proptest::option::of("[a-z][a-z0-9_]{0,8}"),
            tail in "(\\.[a-z][a-z0-9_]{0,8}){0,3}",
        ) {
            prop_assume!(local != module || name.is_some());
            let mut imports = Imports::default();
            match name.as_deref() {
                Some(name) => imports.name(&module, 0, name, Some(&local), 1),
                None => imports.module(&module, Some(&local), 1),
            }
            let full = match name.as_deref() {
                Some(name) => format!("{module}.{name}{tail}"),
                None => format!("{module}{tail}"),
            };
            prop_assert_eq!(rewritten(&imports, &format!("{local}{tail}")), Some(full));
        }

        #[test]
        fn no_name_panics_the_rewriting(name in "\\PC{0,40}") {
            let mut imports = Imports::default();
            imports.module("requests", Some("r"), 1);
            imports.name("os", 0, "environ", None, 2);
            // Whatever the name, a rewritten one is never empty: the module it stands for is not.
            prop_assert!(rewritten(&imports, &name).is_none_or(|out| !out.is_empty()));
        }
    }
}
