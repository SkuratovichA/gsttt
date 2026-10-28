//! Finding the tests that claim scenarios: every `#[scenario]` attribute in the sources that the
//! test targets of the workspace compile.
//!
//! The walk starts at the crate root of each library, binary and test target that cargo runs tests
//! for and follows the `mod` declarations from there, `#[path]` attributes and `#[cfg(test)]`
//! modules included. A `.rs` file that no `mod` reaches is compiled by nothing and an example or a
//! benchmark is no test target, so a test in either never runs. `include!` is not followed.
//!
//! The attribute is a proof on a function that
//!
//! - carries a `test` attribute (any path ending in `test`, such as `tokio::test`) or sits inside a
//!   `proptest!` block, whose functions become tests,
//! - is not `#[ignore]`d, and
//! - is compiled under `cfg(test)` alone: no other `cfg` guards the function, a module around it,
//!   its file, or its target (`required-features`).
//!
//! Anywhere else it proves nothing, and the gate says where it is.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gsttt_spec::scenario_args;
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::ext::IdentExt as _;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned as _;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprLit, Ident, ImplItemFn, ItemFn, ItemMacro, ItemMod, Lit, Macro, Meta,
    MetaNameValue, Token, TraitItemFn,
};

use super::workspace::{Target, Workspace, clean};

/// Directories that hold material for tests, not tests: the attributes in them are no proofs.
const LEFT_OUT: [&str; 2] = ["fixtures", "snapshots"];

/// Why the proofs of a `.rs` file that no target reaches are none.
const UNREACHED: &str =
    "its file is part of no library, binary or test target: no `mod` declaration reaches it";

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

/// Reads the sources that the test targets of `workspace` compile, and the `.rs` files of its
/// members that no target reaches, except those below directories named `fixtures` or
/// `snapshots`.
pub(super) fn find(workspace: &Workspace) -> anyhow::Result<Marks> {
    let roots = workspace
        .packages
        .iter()
        .flat_map(|package| &package.targets)
        .map(Entry::root)
        .collect();
    let mut walk = Walk {
        root: &workspace.root,
        seen: BTreeSet::new(),
        marks: Marks::default(),
    };
    walk.follow(roots)?;
    for package in &workspace.packages {
        let mut files = Vec::new();
        rust_files(&package.dir, &mut files)?;
        for file in files {
            walk.unreached(file)?;
        }
    }
    let mut marks = walk.marks;
    marks.claims.sort();
    marks.misuses.sort();
    Ok(marks)
}

/// A source file to read, and what is known about the way a target reaches it.
struct Entry {
    path: PathBuf,
    /// The directory in which the `mod name;` declarations of the file look for files.
    modules: PathBuf,
    /// Why a proof in the file is none, when it is.
    doubt: Option<String>,
}

impl Entry {
    fn root(target: &Target) -> Self {
        let doubt = (!target.features.is_empty()).then(|| {
            let features: Vec<String> = target
                .features
                .iter()
                .map(|feature| format!("`{feature}`"))
                .collect();
            format!(
                "its target `{}` is built only with the cargo features {}",
                target.name,
                features.join(", ")
            )
        });
        Self {
            path: target.root.clone(),
            modules: target
                .root
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            doubt,
        }
    }
}

/// A `mod name;` declaration, with what is needed to find its file.
struct Declared {
    name: String,
    /// The line of the declaration.
    line: usize,
    /// The file that a `#[path]` attribute names.
    explicit: Option<PathBuf>,
    /// The directory in which a module without `#[path]` looks for `name.rs` and `name/mod.rs`.
    modules: PathBuf,
    doubt: Option<String>,
}

struct Walk<'a> {
    root: &'a Path,
    /// The files read so far, so that a file that two targets declare is read once.
    seen: BTreeSet<PathBuf>,
    marks: Marks,
}

impl Walk<'_> {
    /// Reads the files of `roots` and, in the order of their declarations, the files they declare.
    ///
    /// A file that one target declares under a `cfg` and another declares plainly is compiled for
    /// sure, so the files reached with no doubt are all read before the first one with a doubt.
    fn follow(&mut self, roots: Vec<Entry>) -> anyhow::Result<()> {
        // Stacks: the last pushed is read first, so the first declared goes on last.
        let (mut sure, mut doubtful): (Vec<Entry>, Vec<Entry>) = roots
            .into_iter()
            .rev()
            .partition(|entry| entry.doubt.is_none());
        while let Some(entry) = sure.pop().or_else(|| doubtful.pop()) {
            if !self.seen.insert(entry.path.clone()) {
                continue;
            }
            let text = read(&entry.path)?;
            let declared = self.scan(&entry, &text, true)?;
            for module in declared.into_iter().rev() {
                if let Some(found) = self.locate(&entry.path, module)? {
                    if found.doubt.is_none() {
                        sure.push(found);
                    } else {
                        doubtful.push(found);
                    }
                }
            }
        }
        Ok(())
    }

    /// A `.rs` file of a member that no target declared: whatever it marks proves nothing.
    fn unreached(&mut self, path: PathBuf) -> anyhow::Result<()> {
        if self.seen.contains(&path) {
            return Ok(());
        }
        let text = read(&path)?;
        // Most files never mention a scenario, and parsing them would only cost time.
        if text.contains("scenario") {
            let entry = Entry {
                path,
                modules: PathBuf::new(),
                doubt: Some(UNREACHED.to_owned()),
            };
            self.scan(&entry, &text, false)?;
        }
        Ok(())
    }

    /// Records what one file marks and returns the modules it declares, when `follow` is set.
    fn scan(&mut self, entry: &Entry, text: &str, follow: bool) -> anyhow::Result<Vec<Declared>> {
        let file = self.shown(&entry.path);
        let syntax = syn::parse_file(text).map_err(|error| {
            let start = error.span().start();
            anyhow::anyhow!(
                "{}:{}:{}: cannot read the source: {error}",
                file.display(),
                start.line,
                start.column + 1
            )
        })?;
        let mut scanner = Scanner {
            file: &file,
            marks: &mut self.marks,
            handled: BTreeSet::new(),
            doubt: entry.doubt.clone().or_else(|| doubts(&syntax.attrs)),
            directory: entry
                .path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            modules: entry.modules.clone(),
            inline: 0,
            follow,
            declared: Vec::new(),
        };
        scanner.visit_file(&syntax);
        Ok(scanner.declared)
    }

    /// The file of a module that `declaring` declares with `mod name;`. A module that cannot be
    /// found is an error, unless a `cfg` or a target's features may keep the compiler from looking.
    fn locate(&self, declaring: &Path, module: Declared) -> anyhow::Result<Option<Entry>> {
        let Declared {
            name,
            line,
            explicit,
            modules,
            doubt,
        } = module;
        // A file named by `#[path]` looks for its own modules beside itself, as a `mod.rs` does.
        let candidates = if let Some(path) = explicit {
            let beside = path.parent().map(Path::to_path_buf).unwrap_or_default();
            vec![(path, beside)]
        } else {
            let own = modules.join(&name);
            vec![
                (modules.join(format!("{name}.rs")), own.clone()),
                (own.join("mod.rs"), own),
            ]
        };
        if let Some((path, modules)) = candidates.iter().find(|(path, _)| path.is_file()) {
            return Ok(Some(Entry {
                path: path.clone(),
                modules: modules.clone(),
                doubt,
            }));
        }
        if doubt.is_some() {
            return Ok(None);
        }
        let looked: Vec<String> = candidates
            .iter()
            .map(|(path, _)| self.shown(path).display().to_string())
            .collect();
        let site = Site {
            file: self.shown(declaring),
            line,
        };
        anyhow::bail!(
            "{site}: cannot find the file of module `{name}`; looked for {}",
            looked.join(" and ")
        )
    }

    /// A path named relative to the repository root, as the report names files.
    fn shown(&self, path: &Path) -> PathBuf {
        path.strip_prefix(self.root).unwrap_or(path).to_path_buf()
    }
}

fn read(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

/// The `.rs` files below `dir`, sorted, leaving out what is not a source of the package: build
/// output, hidden directories such as `.git` and `.claude/worktrees`, material for tests, and the
/// directories of other packages, which are members of their own or not ours at all. Without those,
/// a package at the repository root would take every copy of the tree for its own.
fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let listing = || format!("listing {}", dir.display());
    let mut entries = fs::read_dir(dir)
        .with_context(listing)?
        .collect::<io::Result<Vec<_>>>()
        .with_context(listing)?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        // The type of the entry itself, so that a symbolic link is never followed into a cycle.
        let kind = entry
            .file_type()
            .with_context(|| format!("reading the type of {}", path.display()))?;
        if kind.is_dir() {
            if !left_out(&entry.file_name(), &path) {
                rust_files(&path, found)?;
            }
        } else if kind.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

/// Whether the subdirectory `dir` of a package is no source of it.
fn left_out(name: &OsStr, dir: &Path) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.')
        || name == "target"
        || LEFT_OUT.iter().any(|left_out| name == *left_out)
        || dir.join("Cargo.toml").is_file()
}

struct Scanner<'a> {
    file: &'a Path,
    marks: &'a mut Marks,
    /// The attributes judged on a function, by line and column. Any other `#[scenario]` marks
    /// something that is not a function.
    handled: BTreeSet<(usize, usize)>,
    /// Why a proof in the module being read is none, when it is.
    doubt: Option<String>,
    /// The directory of the file, which a `#[path]` outside inline modules is relative to.
    directory: PathBuf,
    /// The directory in which a `mod name;` of the module being read looks for its file.
    modules: PathBuf,
    /// How many inline modules the reading is inside.
    inline: usize,
    /// Whether to note the `mod name;` declarations, for the walk to follow.
    follow: bool,
    declared: Vec<Declared>,
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
        let doubt = self.doubt.clone().or_else(|| doubts(attrs));
        let function_line = name.span().start().line;
        for attr in attrs
            .iter()
            .filter(|attr| last_segment_is(attr, "scenario"))
        {
            self.handled.insert(position(attr));
            let (capability, scenario) = match arguments(attr) {
                Ok(found) => found,
                Err((line, message)) => {
                    self.misuse(line, message);
                    continue;
                }
            };
            if !is_test {
                self.misuse(
                    function_line,
                    format!(
                        "`{name}` is marked with #[scenario] but has no #[test] attribute and is not inside proptest!, so it proves nothing"
                    ),
                );
            } else if let Some(why) = &doubt {
                self.misuse(
                    function_line,
                    format!("`{name}` is marked with #[scenario] but {why}, so it proves nothing"),
                );
            } else {
                let site = self.site(function_line);
                self.marks.claims.push(Claim {
                    capability,
                    scenario,
                    function: name.to_string(),
                    site,
                });
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
                    let line = pound.span().start().line;
                    let inner = tokens.next_if(is_bang).is_some();
                    if let Some(group) = tokens.next_if(is_bracket)
                        && !inner
                    {
                        let attribute = TokenStream::from_iter([TokenTree::Punct(pound), group]);
                        match Attribute::parse_outer.parse2(attribute) {
                            Ok(parsed) => attrs.extend(parsed),
                            // Dropped, it could hide a #[scenario] or an #[ignore] from the gate.
                            Err(error) => self.misuse(
                                line,
                                format!(
                                    "cannot read this attribute inside proptest!, so the gate cannot tell what it marks: {error}"
                                ),
                            ),
                        }
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

    /// Adds the doubt that the attributes of an item bring to the one in force. Returns the one to
    /// put back once the item is read.
    fn enter(&mut self, attrs: &[Attribute]) -> Option<String> {
        let doubt = self.doubt.clone().or_else(|| doubts(attrs));
        std::mem::replace(&mut self.doubt, doubt)
    }

    /// Notes a `mod name;` declaration for the walk to follow.
    fn declare(&mut self, node: &ItemMod, name: String) {
        let explicit = path_attribute(&node.attrs).map(|relative| {
            // Inside inline modules the path starts at the directory of the module; outside them,
            // at the directory of the file.
            let base = if self.inline > 0 {
                &self.modules
            } else {
                &self.directory
            };
            clean(&base.join(relative))
        });
        self.declared.push(Declared {
            name,
            line: node.ident.span().start().line,
            explicit,
            modules: self.modules.clone(),
            doubt: self.doubt.clone(),
        });
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

    fn visit_item_mod(&mut self, node: &'ast ItemMod) {
        let outer = (self.enter(&node.attrs), self.modules.clone());
        let name = node.ident.unraw().to_string();
        let inline = node.content.is_some();
        if inline {
            self.modules.push(&name);
            self.inline += 1;
        } else if self.follow {
            self.declare(node, name);
        }
        visit::visit_item_mod(self, node);
        if inline {
            self.inline -= 1;
        }
        (self.doubt, self.modules) = outer;
    }

    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        let outer = self.enter(&node.attrs);
        visit::visit_item_macro(self, node);
        self.doubt = outer;
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

/// The tokens between the parentheses of an attribute such as `#[name(…)]`; none for the others.
fn list_tokens(attr: &Attribute) -> TokenStream {
    match &attr.meta {
        Meta::List(list) => list.tokens.clone(),
        Meta::Path(_) | Meta::NameValue(_) => TokenStream::new(),
    }
}

/// The tokens as they read in source, without the spaces that printing a token stream adds.
fn spelled(tokens: &TokenStream) -> String {
    tokens
        .to_string()
        .replace(" , ", ", ")
        .replace(" (", "(")
        .replace("( ", "(")
        .replace(" )", ")")
}

/// Why an item with these attributes is no proof, when it is: it is ignored, or compiled only
/// under a `cfg` other than `test`. A `cfg_attr` that ignores the item under some `cfg` counts as an
/// `#[ignore]`: the gate cannot tell that the test runs.
fn doubts(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        let path = attr.path();
        if path.is_ident("ignore") {
            Some("is #[ignore]d".to_owned())
        } else if path.is_ident("cfg") {
            let is_test =
                matches!(attr.parse_args::<Meta>(), Ok(Meta::Path(path)) if path.is_ident("test"));
            (!is_test).then(|| {
                format!(
                    "is compiled only under cfg({})",
                    spelled(&list_tokens(attr))
                )
            })
        } else if path.is_ident("cfg_attr") {
            let ignores = attr
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .is_ok_and(|metas| {
                    metas
                        .iter()
                        .skip(1)
                        .any(|meta| meta.path().is_ident("ignore"))
                });
            ignores.then(|| format!("is ignored under cfg_attr({})", spelled(&list_tokens(attr))))
        } else {
            None
        }
    })
}

/// The file named by the `#[path = "…"]` attribute of a module, as written.
fn path_attribute(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(MetaNameValue {
            path,
            value:
                Expr::Lit(ExprLit {
                    lit: Lit::Str(text),
                    ..
                }),
            ..
        }) if path.is_ident("path") => Some(text.value()),
        _ => None,
    })
}

/// The capability and the scenario of a `#[scenario]` attribute, read by the rule the attribute
/// itself applies, or the line of the problem and what is wrong.
fn arguments(attr: &Attribute) -> Result<(String, String), (usize, String)> {
    let tokens = list_tokens(attr);
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
