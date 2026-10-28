//! Directory arithmetic on repository paths, where `""` is the repository root.

use gsttt_core::path::{PathError, RepoPath};

/// `relative` taken from `dir` as a repository path: `""` for the root, `None` when it leaves the repository.
pub(crate) fn join_dir(dir: &str, relative: &str) -> Option<String> {
    match RepoPath::join(dir, relative) {
        Ok(path) => Some(path.as_str().to_owned()),
        Err(PathError::Empty) => Some(String::new()),
        Err(_) => None,
    }
}

/// The file `name` inside `dir`.
pub(crate) fn join_file(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// The directory part of a repository path, `""` at the root.
pub(crate) fn file_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The last segment of a repository path.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `path` relative to `dir` when it lies under it, so `dist/a.js` under `dist` is `a.js`.
pub(crate) fn strip_dir<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    if dir.is_empty() {
        return Some(path);
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

/// `dir` and each directory above it, ending with the root `""`.
pub(crate) fn ancestors(dir: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(dir), |dir| {
        if dir.is_empty() {
            None
        } else {
            Some(file_dir(dir))
        }
    })
}

/// Whether a specifier is a relative path rather than a package name.
pub(crate) fn is_relative(spec: &str) -> bool {
    spec == "." || spec == ".." || spec.starts_with("./") || spec.starts_with("../")
}

/// Whether `name` is one of the segments of `path`.
pub(crate) fn has_segment(path: &str, name: &str) -> bool {
    path.split('/').any(|segment| segment == name)
}

/// Whether a file name ends in `.json`, exactly as written.
pub(crate) fn is_json(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, extension)| extension == "json")
}

/// Whether a file name is a tsconfig: `tsconfig.json`, `tsconfig.base.json`, `tsconfig.build.json`.
pub(crate) fn is_config_name(name: &str) -> bool {
    name.starts_with("tsconfig") && is_json(name)
}

/// Splits a bare specifier into the package name and the subpath: `@acme/ui/button` is `("@acme/ui", "button")`
/// and `react` is `("react", "")`. A scope with no package, and the empty specifier, name nothing.
pub(crate) fn split_package(spec: &str) -> Option<(&str, &str)> {
    let from = if spec.starts_with('@') {
        spec.find('/')? + 1
    } else {
        0
    };
    match spec.get(from..)?.find('/') {
        Some(offset) => {
            let end = from + offset;
            Some((spec.get(..end)?, spec.get(end + 1..)?))
        }
        None => (!spec.is_empty()).then_some((spec, "")),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn directories_stack_up_to_the_root() {
        assert_eq!(ancestors("a/b").collect::<Vec<_>>(), ["a/b", "a", ""]);
        assert_eq!(ancestors("").collect::<Vec<_>>(), [""]);
    }

    #[test]
    fn joins_stay_inside_the_repository() {
        assert_eq!(join_dir("a/b", "../c").as_deref(), Some("a/c"));
        assert_eq!(join_dir("a", "..").as_deref(), Some(""));
        assert_eq!(join_dir("a", "../..").as_deref(), None);
        assert_eq!(strip_dir("dist/a.js", "dist"), Some("a.js"));
        assert_eq!(strip_dir("distant/a.js", "dist"), None);
    }

    #[test]
    fn packages_split_at_the_scope_aware_slash() {
        assert_eq!(split_package("react"), Some(("react", "")));
        assert_eq!(
            split_package("react-dom/client"),
            Some(("react-dom", "client"))
        );
        assert_eq!(split_package("@acme/ui"), Some(("@acme/ui", "")));
        assert_eq!(split_package("@acme/ui/a/b"), Some(("@acme/ui", "a/b")));
        assert_eq!(split_package("@acme"), None);
        assert_eq!(split_package(""), None);
    }

    proptest! {
        #[test]
        fn a_split_package_joins_back_to_its_specifier(spec in "[@a-z/._-]{0,24}") {
            if let Some((name, subpath)) = split_package(&spec) {
                // After the name comes nothing, or a slash and the subpath (which `a/` leaves empty).
                let slash_and_subpath = format!("/{subpath}");
                let rest = spec.strip_prefix(name).unwrap_or(&spec);
                prop_assert!(rest.is_empty() || rest == slash_and_subpath);
            }
        }

        #[test]
        fn joins_never_panic(dir in "[a-z/.]{0,12}", relative in "[a-z/.]{0,12}") {
            drop(join_dir(&dir, &relative));
            let _ = strip_dir(&relative, &dir);
        }
    }
}
