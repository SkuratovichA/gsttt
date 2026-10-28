//! Finding the tests that claim scenarios: every `#[scenario]` attribute in the `.rs` files of the
//! workspace's members.
//!
//! The attribute is a proof on a function that also carries a `test` attribute (any path ending in
//! `test`, such as `tokio::test`) or that sits inside a `proptest!` block, whose functions become
//! tests. Anywhere else it proves nothing, and the gate says where it is.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gsttt_spec::scenario_args;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::parse::Parser as _;
use syn::spanned::Spanned as _;
use syn::visit::{self, Visit};
use syn::{Attribute, Ident, ImplItemFn, ItemFn, Macro, Meta, TraitItemFn};

/// Directories that hold material for tests, not tests: the attributes in them are no proofs.
const LEFT_OUT: [&str; 2] = ["fixtures", "snapshots"];

/// A line of a file, named relative to the repository root.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Site {
    pub(super) file: PathBuf,
    pub(super) line: usize,
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file.display(), self.line)
    }
}

/// A test that claims a scenario with `#[scenario]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Claim {
    pub(super) capability: String,
    pub(super) scenario: String,
    /// The name of the marked function.
    pub(super) function: String,
    /// The line of the marked function.
    pub(super) site: Site,
}

impl fmt::Display for Claim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} `{}`", self.site, self.function)
    }
}

/// A `#[scenario]` attribute that proves nothing, and why.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Misuse {
    pub(super) site: Site,
    pub(super) message: String,
}

/// What the sources of the workspace claim, sorted.
#[derive(Debug, Default)]
pub(super) struct Marks {
    pub(super) claims: Vec<Claim>,
    pub(super) misuses: Vec<Misuse>,
}

/// Reads the `.rs` files under each member directory (absolute, or relative to `root`), except
/// those below directories named `fixtures` or `snapshots`.
pub(super) fn find(root: &Path, members: &[PathBuf]) -> anyhow::Result<Marks> {
    let mut marks = Marks::default();
    for member in members {
        let mut files = Vec::new();
        rust_files(&root.join(member), &mut files)
            .with_context(|| format!("listing the sources of {}", member.display()))?;
        for path in files {
            let text =
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            // Most files never mention a scenario, and parsing them would only cost time.
            if text.contains("scenario") {
                scan(path.strip_prefix(root).unwrap_or(&path), &text, &mut marks)?;
            }
        }
    }
    marks.claims.sort();
    marks.misuses.sort();
    Ok(marks)
}

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        // The type of the entry itself, so that a symbolic link is never followed into a cycle.
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            let name = entry.file_name();
            if !LEFT_OUT.iter().any(|left_out| name == *left_out) {
                rust_files(&path, found)?;
            }
        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

fn scan(file: &Path, text: &str, marks: &mut Marks) -> anyhow::Result<()> {
    let syntax = syn::parse_file(text).map_err(|error| {
        let start = error.span().start();
        anyhow::anyhow!(
            "{}:{}:{}: cannot read the source: {error}",
            file.display(),
            start.line,
            start.column + 1
        )
    })?;
    Scanner {
        file,
        marks,
        handled: BTreeSet::new(),
    }
    .visit_file(&syntax);
    Ok(())
}

struct Scanner<'a> {
    file: &'a Path,
    marks: &'a mut Marks,
    /// The attributes judged on a function, by line and column. Any other `#[scenario]` marks
    /// something that is not a function.
    handled: BTreeSet<(usize, usize)>,
}

impl Scanner<'_> {
    fn site(&self, line: usize) -> Site {
        Site {
            file: self.file.to_path_buf(),
            line,
        }
    }

    fn misuse(&mut self, line: usize, message: String) {
        let site = self.site(line);
        self.marks.misuses.push(Misuse { site, message });
    }

    /// Judges the `#[scenario]` attributes of one function.
    fn function(&mut self, attrs: &[Attribute], name: &Ident, in_proptest: bool) {
        let is_test = in_proptest || attrs.iter().any(|attr| last_segment_is(attr, "test"));
        let function_line = name.span().start().line;
        for attr in attrs
            .iter()
            .filter(|attr| last_segment_is(attr, "scenario"))
        {
            self.handled.insert(position(attr));
            match arguments(attr) {
                Err((line, message)) => self.misuse(line, message),
                Ok((capability, scenario)) if is_test => {
                    let site = self.site(function_line);
                    self.marks.claims.push(Claim {
                        capability,
                        scenario,
                        function: name.to_string(),
                        site,
                    });
                }
                Ok(_) => self.misuse(
                    function_line,
                    format!(
                        "`{name}` is marked with #[scenario] but has no #[test] attribute and is not inside proptest!, so it proves nothing"
                    ),
                ),
            }
        }
    }

    /// The functions of a `proptest!` block, with the attributes in front of them. The block is not
    /// Rust syntax (`name in strategy` parameters), so it is read as tokens.
    fn proptest(&mut self, body: TokenStream) {
        let mut attrs: Vec<Attribute> = Vec::new();
        let mut tokens = body.into_iter().peekable();
        while let Some(tree) = tokens.next() {
            match tree {
                TokenTree::Punct(pound) if pound.as_char() == '#' => {
                    let inner = tokens.next_if(is_bang).is_some();
                    if let Some(group) = tokens.next_if(is_bracket)
                        && !inner
                    {
                        let attribute = TokenStream::from_iter([TokenTree::Punct(pound), group]);
                        attrs.extend(Attribute::parse_outer.parse2(attribute).unwrap_or_default());
                    }
                }
                TokenTree::Ident(keyword) if keyword == "fn" => {
                    if let Some(TokenTree::Ident(name)) = tokens.next() {
                        self.function(&attrs, &name, true);
                    }
                    attrs.clear();
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        self.function(&node.attrs, &node.sig.ident, false);
        visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        self.function(&node.attrs, &node.sig.ident, false);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        self.function(&node.attrs, &node.sig.ident, false);
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        if node
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "proptest")
        {
            self.proptest(node.tokens.clone());
        }
        visit::visit_macro(self, node);
    }

    fn visit_attribute(&mut self, node: &'ast Attribute) {
        if last_segment_is(node, "scenario") && !self.handled.contains(&position(node)) {
            self.misuse(
                node.span().start().line,
                "#[scenario] marks something that is not a function".to_owned(),
            );
        }
    }
}

fn last_segment_is(attr: &Attribute, name: &str) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

/// Where the attribute starts, which names it within its file.
fn position(attr: &Attribute) -> (usize, usize) {
    let start = attr.span().start();
    (start.line, start.column)
}

/// The capability and the scenario of a `#[scenario]` attribute, read by the rule the attribute
/// itself applies, or the line of the problem and what is wrong.
fn arguments(attr: &Attribute) -> Result<(String, String), (usize, String)> {
    let tokens = match &attr.meta {
        Meta::List(list) => list.tokens.clone(),
        Meta::Path(_) | Meta::NameValue(_) => TokenStream::new(),
    };
    let empty = tokens.is_empty();
    scenario_args(tokens).map_err(|error| {
        // With no argument to point at, the error belongs to the attribute.
        let line = if empty {
            attr.span().start().line
        } else {
            error.span().start().line
        };
        (line, error.to_string())
    })
}

fn is_bang(tree: &TokenTree) -> bool {
    matches!(tree, TokenTree::Punct(punct) if punct.as_char() == '!')
}

fn is_bracket(tree: &TokenTree) -> bool {
    matches!(tree, TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket)
}
