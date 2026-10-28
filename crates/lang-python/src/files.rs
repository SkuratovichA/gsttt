//! Which files a Python project reads, and how it tags them.

use gsttt_core::fact::FileTags;
use gsttt_core::path::{PathError, RepoPath};

/// `relative` taken from the repository directory `dir` (`""` is the root), as a repository directory. `None`
/// when it leaves the repository.
pub(crate) fn join_dir(dir: &str, relative: &str) -> Option<String> {
    match RepoPath::join(dir, relative) {
        Ok(path) => Some(path.as_str().to_owned()),
        // The root itself: `.` or a `..` that cancels the whole directory.
        Err(PathError::Empty) => Some(String::new()),
        Err(PathError::Absolute(_) | PathError::EscapesRoot(_)) => None,
    }
}

/// The files whose directory becomes a project.
pub(crate) const PROJECT_MANIFESTS: [&str; 3] = ["pyproject.toml", "setup.py", "setup.cfg"];

/// Whether `path` is Python source or a stub.
pub(crate) fn is_python_source(path: &RepoPath) -> bool {
    matches!(path.extension(), Some("py" | "pyi"))
}

/// `requirements.txt`, `requirements-dev.txt` and the like, and the text files of a directory named `requirements`.
pub(crate) fn is_requirements(path: &RepoPath) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        && (path.file_name().starts_with("requirements")
            || path.parent().rsplit('/').next() == Some("requirements"))
}

/// A file a project claims as its manifest: a project file or a requirements file.
pub(crate) fn is_manifest(path: &RepoPath) -> bool {
    PROJECT_MANIFESTS.contains(&path.file_name()) || is_requirements(path)
}

/// Files named `test_*.py` or `*_test.py`, `conftest.py`, and files under a directory named `tests` or `test`.
pub(crate) fn is_test_path(path: &RepoPath) -> bool {
    let name = path.file_name();
    let named_as_test = name.strip_suffix(".py").is_some_and(|stem| {
        stem.starts_with("test_") || stem.ends_with("_test") || stem == "conftest"
    });
    named_as_test
        || path
            .as_str()
            .rsplit('/')
            .skip(1)
            .any(|directory| directory == "tests" || directory == "test")
}

/// The tags of a file the project claims.
pub(crate) fn tags_of(path: &RepoPath) -> FileTags {
    let mut tags = FileTags::NONE;
    if is_test_path(path) {
        tags = tags | FileTags::TEST;
    }
    if is_manifest(path) {
        tags = tags | FileTags::MANIFEST;
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RepoPath {
        RepoPath::normalize(text).unwrap()
    }

    #[test]
    fn tests_are_named_by_file_or_by_directory() {
        for text in [
            "tests/helpers.py",
            "app/test_api.py",
            "app/api_test.py",
            "app/conftest.py",
            "svc/test/util.py",
        ] {
            assert!(is_test_path(&path(text)), "{text}");
        }
        for text in ["app/api.py", "app/testing.py", "app/tests.py", "contest.py"] {
            assert!(!is_test_path(&path(text)), "{text}");
        }
    }

    #[test]
    fn manifests_are_the_project_files_and_requirements() {
        for name in [
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "requirements-dev.txt",
            "svc/requirements/base.txt",
            "requirements/Dev.TXT",
        ] {
            assert!(is_manifest(&path(name)), "{name}");
        }
        for name in [
            "README.md",
            "dev-requirements.txt",
            "requirements/base.md",
            "requirements/nested/base.txt",
            "my-requirements/base.txt",
            "base.txt",
        ] {
            assert!(!is_manifest(&path(name)), "{name}");
        }
    }
}
