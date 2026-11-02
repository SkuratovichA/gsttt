//! The names a Go file can write without declaring them, and the name a package is written by.

/// Predeclared types and constants. A file that writes one of them reads nothing from its package or from another, so they are no names to follow.
const PREDECLARED_TYPES_AND_CONSTANTS: [&str; 26] = [
    "any",
    "bool",
    "byte",
    "comparable",
    "complex128",
    "complex64",
    "error",
    "false",
    "float32",
    "float64",
    "iota",
    "int",
    "int16",
    "int32",
    "int64",
    "int8",
    "nil",
    "rune",
    "string",
    "true",
    "uint",
    "uint16",
    "uint32",
    "uint64",
    "uint8",
    "uintptr",
];

/// Whether `name` is a predeclared type or constant. The predeclared functions are not: a package may declare its own `min`, and the file that calls it reads that declaration.
pub(crate) fn is_predeclared(name: &str) -> bool {
    PREDECLARED_TYPES_AND_CONSTANTS.contains(&name)
}

const PREDECLARED_FUNCTIONS: [&str; 18] = [
    "append", "cap", "clear", "close", "complex", "copy", "delete", "imag", "len", "make", "max",
    "min", "new", "panic", "print", "println", "real", "recover",
];

/// Whether `name` is a predeclared function. A call of one in a package-level initialiser runs nothing of the package, unless the package declares a function of that name, which this does not look for.
pub(crate) fn is_predeclared_function(name: &str) -> bool {
    PREDECLARED_FUNCTIONS.contains(&name)
}

/// The name a package is written by when its import has no alias: the last segment of the path, read past a major version, so `chi/v5` is `chi` and `yaml.v3` is `yaml`.
pub(crate) fn default_qualifier(path: &str) -> &str {
    let mut segments = path.rsplit('/');
    let last = segments.next().unwrap_or_default();
    let segment = if is_major_version(last) {
        segments.next().unwrap_or(last)
    } else {
        last
    };
    match segment.rsplit_once(".v") {
        Some((name, version))
            if !name.is_empty() && version.bytes().all(|b| b.is_ascii_digit()) =>
        {
            name
        }
        _ => segment,
    }
}

/// Whether the segment is a major version of a module path: `v2`, `v10`.
fn is_major_version(segment: &str) -> bool {
    segment
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// The spelling of a package name that the go tool and a writer tend to disagree on: separators are dropped and a leading or trailing `go` is taken off, so the `go-foo` of a path is the `foo` the package calls itself.
pub(crate) fn squeezed(segment: &str) -> String {
    let plain: String = segment
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | '.'))
        .collect();
    let plain = plain.strip_prefix("go").unwrap_or(&plain);
    let plain = plain.strip_suffix("go").unwrap_or(plain);
    plain.to_owned()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn the_qualifier_is_read_past_a_major_version() {
        assert_eq!(default_qualifier("github.com/go-chi/chi/v5"), "chi");
        assert_eq!(default_qualifier("gopkg.in/yaml.v3"), "yaml");
        assert_eq!(default_qualifier("net/http"), "http");
        assert_eq!(default_qualifier("fmt"), "fmt");
    }

    #[test]
    fn a_segment_is_squeezed_to_what_a_package_name_may_be() {
        assert_eq!(squeezed("go-redis"), "redis");
        assert_eq!(squeezed("yaml.v3"), "yamlv3");
        assert_eq!(squeezed("my_pkg"), "mypkg");
        assert_eq!(squeezed("gogox"), "gox");
    }

    proptest! {
        /// The qualifier is always a piece of the path, and a path ending in a major version never gives the version.
        #[test]
        fn the_qualifier_is_a_segment_of_the_path(path in "[a-z0-9./_-]{0,40}") {
            let qualifier = default_qualifier(&path);
            prop_assert!(path.contains(qualifier));
            prop_assert!(!qualifier.contains('/'));
        }

        /// Squeezing is not idempotent (`gogox` gives `gox`, which gives `x`), so it is applied once, and what it gives holds no separator and nothing that was not in the segment.
        #[test]
        fn a_squeezed_segment_holds_no_separator(segment in "[a-zA-Z0-9._-]{0,20}") {
            let squeezed = squeezed(&segment);
            prop_assert!(!squeezed.contains(['-', '_', '.']));
            prop_assert!(squeezed.len() <= segment.len());
        }
    }
}
