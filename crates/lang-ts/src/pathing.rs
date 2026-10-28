//! Directory arithmetic on repository paths, where `""` is the repository root.

/// The file `name` inside `dir`.
pub(crate) fn join_file(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// The directory part of a path kept as text: the parent of a directory too, which reaches the root `""` that a
/// [`RepoPath`] cannot name. For a path that is a `RepoPath`, `RepoPath::parent` is the same function, and a test pins
/// the two together.
pub(crate) fn file_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
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

/// Whether a file name is a config of the compiler: `tsconfig.json`, `tsconfig.base.json`, `tsconfig.build.json`, or the
/// `jsconfig.json` of a JavaScript project.
pub(crate) fn is_config_name(name: &str) -> bool {
    (name.starts_with("tsconfig") || name.starts_with("jsconfig")) && is_json(name)
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
    use gsttt_core::path::RepoPath;
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn directories_stack_up_to_the_root() {
        assert_eq!(ancestors("a/b").collect::<Vec<_>>(), ["a/b", "a", ""]);
        assert_eq!(ancestors("").collect::<Vec<_>>(), [""]);
    }

    #[test]
    fn a_directory_is_stripped_by_whole_segments() {
        assert_eq!(strip_dir("dist/a.js", "dist"), Some("a.js"));
        assert_eq!(strip_dir("distant/a.js", "dist"), None);
    }

    #[test]
    fn configs_are_tsconfigs_and_jsconfigs() {
        assert!(is_config_name("tsconfig.json") && is_config_name("tsconfig.app.json"));
        assert!(is_config_name("jsconfig.json"));
        assert!(!is_config_name("package.json") && !is_config_name("tsconfig.txt"));
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
        fn stripping_a_directory_never_panics(dir in "[a-z/.]{0,12}", path in "[a-z/.]{0,12}") {
            let _ = strip_dir(&path, &dir);
        }

        #[test]
        fn the_directory_of_a_path_is_its_parent(raw in "[a-z]{1,3}(/[a-z.]{1,3}){0,4}") {
            if let Ok(path) = RepoPath::normalize(&raw) {
                prop_assert_eq!(file_dir(path.as_str()), path.parent());
            }
        }

        #[test]
        fn lying_under_a_directory_is_the_same_test_as_stripping_it(
            raw in "[a-z]{1,3}(/[a-z.]{1,3}){0,4}",
            dir in "[a-z]{1,3}(/[a-z]{1,3}){0,2}",
        ) {
            if let Ok(path) = RepoPath::normalize(&raw) {
                prop_assert_eq!(strip_dir(path.as_str(), &dir).is_some(), path.is_under(&dir));
            }
        }
    }
}
