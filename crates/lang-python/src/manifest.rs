//! The manifests of a Python project, read for the two things the connector needs from them: the distributions
//! the project declares (they switch rule packs on and tell a dependency from an unknown module) and the
//! directories its package settings name as source roots.

use std::collections::BTreeSet;

use toml::{Table, Value};

use crate::names::{normalize_distribution, requirement_name};

/// What one project's manifests say.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Manifest {
    /// The declared distributions, normalised.
    pub(crate) dependencies: BTreeSet<String>,
    /// Source roots named by package settings, as written and relative to the manifest's directory.
    pub(crate) roots: Vec<String>,
}

impl Manifest {
    /// Adds what `other` declares.
    pub(crate) fn absorb(&mut self, other: Self) {
        self.dependencies.extend(other.dependencies);
        for root in other.roots {
            if !self.roots.contains(&root) {
                self.roots.push(root);
            }
        }
    }

    fn require(&mut self, requirement: &str) {
        if let Some(name) = requirement_name(requirement) {
            self.dependencies.insert(name);
        }
    }

    fn root(&mut self, root: &str) {
        let root = root.trim().trim_start_matches("./").trim_end_matches('/');
        let root = if root == "." { "" } else { root };
        if !self.roots.iter().any(|known| known == root) {
            self.roots.push(root.to_owned());
        }
    }
}

/// A manifest that cannot be read. The project still opens, without what the manifest would have said.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ManifestError {
    #[error("`{file}` is not valid TOML: {message}")]
    Toml {
        file: String,
        /// Where the TOML parser found the error, `0` when it did not say.
        offset: usize,
        message: String,
    },
}

/// Reads `pyproject.toml`: `[project]` dependencies and optional dependencies, dependency groups, Poetry and
/// PDM and uv dependency tables, and the package settings of setuptools, Poetry, Hatch and PDM.
pub(crate) fn read_pyproject(file: &str, text: &str) -> Result<Manifest, ManifestError> {
    let table: Table = text
        .parse()
        .map_err(|error: toml::de::Error| ManifestError::Toml {
            file: file.to_owned(),
            offset: error.span().map_or(0, |span| span.start),
            message: error.message().to_owned(),
        })?;
    let mut manifest = Manifest::default();
    let at = |path: &[&str]| lookup(&table, path);

    for requirement in strings(at(&["project", "dependencies"])) {
        manifest.require(requirement);
    }
    for value in tables_values(at(&["project", "optional-dependencies"])) {
        for requirement in strings(Some(value)) {
            manifest.require(requirement);
        }
    }
    for value in tables_values(at(&["dependency-groups"])) {
        for requirement in strings(Some(value)) {
            manifest.require(requirement);
        }
    }
    for requirement in strings(at(&["tool", "uv", "dev-dependencies"])) {
        manifest.require(requirement);
    }
    for value in tables_values(at(&["tool", "pdm", "dev-dependencies"])) {
        for requirement in strings(Some(value)) {
            manifest.require(requirement);
        }
    }
    for path in [
        &["tool", "poetry", "dependencies"][..],
        &["tool", "poetry", "dev-dependencies"][..],
    ] {
        poetry_names(&mut manifest, at(path));
    }
    for group in tables_values(at(&["tool", "poetry", "group"])) {
        poetry_names(&mut manifest, lookup_value(group, &["dependencies"]));
    }

    setuptools_roots(&mut manifest, &at);
    for package in tables_array(at(&["tool", "poetry", "packages"])) {
        manifest.root(package.get("from").and_then(Value::as_str).unwrap_or(""));
    }
    for package in strings(at(&[
        "tool", "hatch", "build", "targets", "wheel", "packages",
    ])) {
        manifest.root(parent_dir(package));
    }
    for source in strings(at(&["tool", "hatch", "build", "sources"])) {
        manifest.root(source);
    }
    if let Some(dir) = at(&["tool", "pdm", "build", "package-dir"]).and_then(Value::as_str) {
        manifest.root(dir);
    }
    Ok(manifest)
}

fn setuptools_roots<'a>(manifest: &mut Manifest, at: &impl Fn(&[&str]) -> Option<&'a Value>) {
    if let Some(table) = at(&["tool", "setuptools", "package-dir"]).and_then(Value::as_table) {
        for (package, dir) in table {
            if let Some(dir) = dir.as_str() {
                manifest.root(package_root(package, dir));
            }
        }
    }
    let find = at(&["tool", "setuptools", "packages", "find", "where"]);
    match find {
        Some(Value::String(dir)) => manifest.root(dir),
        other => {
            for dir in strings(other) {
                manifest.root(dir);
            }
        }
    }
}

/// Poetry lists its dependencies as table keys; `python` names the interpreter, not a distribution.
fn poetry_names(manifest: &mut Manifest, dependencies: Option<&Value>) {
    let Some(table) = dependencies.and_then(Value::as_table) else {
        return;
    };
    for name in table.keys().filter(|name| name.as_str() != "python") {
        manifest.dependencies.insert(normalize_distribution(name));
    }
}

/// Reads `setup.cfg`: `install_requires`, the keys of `extras_require`, `package_dir` and `packages.find` `where`.
pub(crate) fn read_setup_cfg(text: &str) -> Manifest {
    let mut manifest = Manifest::default();
    let mut section = String::new();
    let mut key = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(['#', ';']) {
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            name.trim().clone_into(&mut section);
            key.clear();
            continue;
        }
        let value = if line.starts_with(char::is_whitespace) {
            trimmed
        } else if let Some((name, value)) = trimmed.split_once(['=', ':']) {
            name.trim().clone_into(&mut key);
            value.trim()
        } else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match (section.as_str(), key.as_str()) {
            ("options", "install_requires") | ("options.extras_require", _) => {
                manifest.require(value);
            }
            ("options", "package_dir") => {
                if let Some((package, dir)) = value.split_once('=') {
                    manifest.root(package_root(package.trim(), dir.trim()));
                }
            }
            ("options.packages.find", "where") => manifest.root(value),
            _ => {}
        }
    }
    manifest
}

/// Reads `setup.py` without running it: the quoted strings of `install_requires=[…]` and the `""` entry of
/// `package_dir={…}`. Anything computed is not read.
pub(crate) fn read_setup_py(text: &str) -> Manifest {
    let mut manifest = Manifest::default();
    for requirement in quoted_in(text, "install_requires", '[', ']') {
        manifest.require(&requirement);
    }
    let strings = quoted_in(text, "package_dir", '{', '}');
    let (pairs, _) = strings.as_chunks::<2>();
    for [package, dir] in pairs {
        manifest.root(package_root(package, dir));
    }
    manifest
}

/// Reads a `requirements*.txt`: one requirement per line, options and comments skipped.
pub(crate) fn read_requirements(text: &str) -> Manifest {
    let mut manifest = Manifest::default();
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or(line).trim();
        if !line.starts_with('#') {
            manifest.require(line);
        }
    }
    manifest
}

/// The directory a `package_dir` entry makes a source root: the entry `"" = "src"` makes `src`, and
/// `"pkg" = "lib/pkg"` makes `lib`, the directory that holds the package.
fn package_root<'a>(package: &str, dir: &'a str) -> &'a str {
    if package.is_empty() {
        return dir;
    }
    dir.strip_suffix(package)
        .map_or(dir, |parent| parent.trim_end_matches('/'))
}

fn parent_dir(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit_once('/')
        .map_or("", |(parent, _)| parent)
}

fn lookup<'a>(table: &'a Table, path: &[&str]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    rest.iter()
        .try_fold(table.get(*first)?, |value, key| value.as_table()?.get(*key))
}

fn lookup_value<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |value, key| value.as_table()?.get(*key))
}

fn strings(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn tables_values(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value
        .and_then(Value::as_table)
        .into_iter()
        .flat_map(Table::values)
}

fn tables_array(value: Option<&Value>) -> impl Iterator<Item = &Table> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_table)
}

/// The contents of every single- or double-quoted string, without escapes, between the first `open` after `key` and
/// the `close` that matches it. A bracket inside a string belongs to the string, so `"fastapi[standard]>=0.1"` is one
/// requirement, and a comment is skipped whole, apostrophe and bracket included. A text that ends before the bracket
/// closes gives the strings read so far.
fn quoted_in(text: &str, key: &str, open: char, close: char) -> Vec<String> {
    let after = text
        .find(key)
        .and_then(|at| text.get(at + key.len()..))
        .unwrap_or_default();
    let inner = after
        .find(open)
        .and_then(|at| after.get(at + open.len_utf8()..))
        .unwrap_or_default();
    let mut found = Vec::new();
    let mut depth = 0_usize;
    let mut quote: Option<(char, String)> = None;
    let mut comment = false;
    for ch in inner.chars() {
        if comment {
            comment = ch != '\n';
            continue;
        }
        match &mut quote {
            Some((mark, content)) if ch == *mark => {
                found.push(std::mem::take(content));
                quote = None;
            }
            Some((_, content)) => content.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some((ch, String::new())),
            None if ch == '#' => comment = true,
            None if ch == open => depth += 1,
            None if ch == close => match depth.checked_sub(1) {
                Some(outer) => depth = outer,
                None => break,
            },
            None => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn deps(manifest: &Manifest) -> Vec<&str> {
        manifest.dependencies.iter().map(String::as_str).collect()
    }

    #[test]
    fn pyproject_dependencies_come_from_every_table() {
        let manifest = read_pyproject(
            "pyproject.toml",
            r#"
            [project]
            dependencies = ["Flask>=3", "PyYAML"]
            [project.optional-dependencies]
            db = ["SQLAlchemy[asyncio]>=2"]
            [dependency-groups]
            dev = ["pytest", { include-group = "lint" }]
            [tool.poetry.dependencies]
            python = "^3.12"
            Django = "^5"
            [tool.poetry.group.test.dependencies]
            httpx = "*"
            "#,
        )
        .unwrap();
        assert_eq!(
            deps(&manifest),
            ["django", "flask", "httpx", "pytest", "pyyaml", "sqlalchemy"]
        );
    }

    #[test]
    fn pyproject_package_settings_name_source_roots() {
        let manifest = read_pyproject(
            "pyproject.toml",
            r#"
            [tool.setuptools.package-dir]
            "" = "src"
            [tool.setuptools.packages.find]
            where = ["lib"]
            [tool.poetry]
            packages = [{ include = "booking", from = "app_src" }, { include = "x" }]
            [tool.hatch.build.targets.wheel]
            packages = ["backend/booking"]
            "#,
        )
        .unwrap();
        assert_eq!(manifest.roots, ["src", "lib", "app_src", "", "backend"]);
    }

    #[test]
    fn a_broken_pyproject_names_the_file() {
        let error = read_pyproject("services/api/pyproject.toml", "[project").unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("`services/api/pyproject.toml` is not valid TOML: "),
            "{error}"
        );
    }

    #[test]
    fn setup_cfg_reads_continuation_lines() {
        let manifest = read_setup_cfg(
            "[options]\npackage_dir =\n    =src\ninstall_requires =\n    Flask>=3\n    requests\n\n[options.extras_require]\ndev =\n    pytest\n[options.packages.find]\nwhere = src\n",
        );
        assert_eq!(deps(&manifest), ["flask", "pytest", "requests"]);
        assert_eq!(manifest.roots, ["src"]);
    }

    #[test]
    fn setup_py_reads_literal_lists() {
        let manifest = read_setup_py(
            "setup(name='x', package_dir={'': 'src'}, install_requires=['Flask>=3', \"requests\"])",
        );
        assert_eq!(deps(&manifest), ["flask", "requests"]);
        assert_eq!(manifest.roots, ["src"]);
    }

    #[test]
    fn setup_py_keeps_the_brackets_of_a_requirement() {
        let manifest = read_setup_py(
            r#"setup(install_requires=["fastapi[standard]>=0.1","sqlalchemy"], extras_require={"dev": ["pytest"]})"#,
        );
        assert_eq!(deps(&manifest), ["fastapi", "sqlalchemy"]);
    }

    #[test]
    fn setup_py_skips_the_comments_of_a_list() {
        let manifest = read_setup_py(
            "setup(install_requires=[\n    'flask',  # don't pin ]\n    \"requests\",\n], name='x')",
        );
        assert_eq!(deps(&manifest), ["flask", "requests"]);
    }

    #[test]
    fn setup_py_reads_the_list_to_its_own_bracket() {
        let manifest = read_setup_py(
            "setup(install_requires=['a', ['b'], 'c'], classifiers=['d [e]'], package_dir={'': 'src'})",
        );
        assert_eq!(deps(&manifest), ["a", "b", "c"]);
        assert_eq!(manifest.roots, ["src"]);
    }

    proptest! {
        #[test]
        fn setup_py_reads_every_requirement_whatever_its_extras(
            requirements in proptest::collection::vec(
                (
                    "[a-z][a-z0-9]{0,8}",
                    proptest::option::of("[a-z]{1,6}(,[a-z]{1,6}){0,2}"),
                    "(>=[0-9]\\.[0-9])?",
                    proptest::bool::ANY,
                ),
                1..6,
            )
        ) {
            let listed: Vec<String> = requirements
                .iter()
                .map(|(name, extras, version, double)| {
                    let extras = extras.as_ref().map_or_else(String::new, |extras| format!("[{extras}]"));
                    let quote = if *double { '"' } else { '\'' };
                    format!("{quote}{name}{extras}{version}{quote}")
                })
                .collect();
            let text = format!("setup(install_requires=[{}], classifiers=['x [y]'])", listed.join(", "));
            let expected: BTreeSet<String> = requirements.iter().map(|(name, ..)| name.clone()).collect();
            prop_assert_eq!(read_setup_py(&text).dependencies, expected);
        }
    }

    #[test]
    fn requirements_skip_options_comments_and_urls() {
        let manifest = read_requirements(
            "# base\n-r other.txt\nflask==3.0  # web\n\nPyYAML\ngit+https://example.com/x.git\n--index-url https://example.com\n",
        );
        assert_eq!(deps(&manifest), ["flask", "pyyaml"]);
    }
}
