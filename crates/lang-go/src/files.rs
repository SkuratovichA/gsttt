//! Which files a Go project reads, and how it tags them.

use gsttt_core::fact::FileTags;
use gsttt_core::path::RepoPath;

/// The file whose directory becomes a project.
pub(crate) const MANIFEST: &str = "go.mod";

/// Whether the go tool leaves `path` out of a package: it skips directories named `testdata` and directories and files that start with `_` or `.`, and the code of `vendor` is another module's. A file that sits in such a place is not the repository's own code.
pub(crate) fn is_excluded(path: &RepoPath) -> bool {
    let mut segments = path.as_str().split('/');
    let name = segments.next_back().unwrap_or_default();
    name.starts_with(['_', '.'])
        || segments.any(|directory| {
            matches!(directory, "vendor" | "testdata") || directory.starts_with(['_', '.'])
        })
}

/// The package a file left out of every build stands for: no sibling is in it.
pub(crate) const NO_PACKAGE: &str = "-";

/// Whether `path` is a Go source file the connector reads.
pub(crate) fn is_go_source(path: &RepoPath) -> bool {
    path.extension() == Some("go") && !is_excluded(path)
}

/// Whether `path` is the `go.mod` of a module the connector reads.
pub(crate) fn is_manifest(path: &RepoPath) -> bool {
    path.file_name() == MANIFEST && !is_excluded(path)
}

/// Whether the file is a test of its package: the go tool compiles `*_test.go` for `go test` only.
pub(crate) fn is_test_name(name: &str) -> bool {
    name.ends_with("_test.go")
}

/// The tags of a file the project claims.
pub(crate) fn tags_of(path: &RepoPath) -> FileTags {
    if is_manifest(path) {
        FileTags::MANIFEST
    } else if is_test_name(path.file_name()) {
        FileTags::TEST
    } else {
        FileTags::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RepoPath {
        RepoPath::normalize(text).unwrap()
    }

    #[test]
    fn vendored_code_test_data_and_hidden_places_are_left_out() {
        for text in [
            "vendor/x/x.go",
            "a/vendor/x.go",
            "a/testdata/x.go",
            "_gen/x.go",
            "a/.hidden/x.go",
            "a/_x.go",
            "a/.x.go",
            "vendor/x/go.mod",
        ] {
            assert!(is_excluded(&path(text)), "{text}");
        }
        for text in ["a/x.go", "a/vendorish/x.go", "a/test_data/x.go", "go.mod"] {
            assert!(!is_excluded(&path(text)), "{text}");
        }
    }

    #[test]
    fn only_go_files_and_go_mod_are_read() {
        assert!(is_go_source(&path("a/x.go")));
        assert!(!is_go_source(&path("a/x.gox")));
        assert!(!is_go_source(&path("a/go.mod")));
        assert!(is_manifest(&path("a/go.mod")));
        assert!(!is_manifest(&path("a/go.sum")));
    }

    #[test]
    fn test_files_end_in_test_go() {
        assert_eq!(tags_of(&path("a/x_test.go")), FileTags::TEST);
        assert_eq!(tags_of(&path("a/x.go")), FileTags::NONE);
        assert_eq!(tags_of(&path("a/go.mod")), FileTags::MANIFEST);
    }
}
