//! Glob sets for the options and settings that list file patterns.

use globset::{Glob, GlobSet, GlobSetBuilder};

/// A list of globs that is not a glob set.
#[derive(Debug, thiserror::Error)]
pub enum GlobError {
    /// One pattern is not a valid glob.
    #[error("{pattern:?} is not a valid glob: {source}")]
    Invalid {
        /// The pattern as it was written.
        pattern: String,
        /// Why it does not parse.
        source: globset::Error,
    },
    /// Every pattern is valid, but the set cannot be built from them.
    #[error("the globs do not build: {0}")]
    Build(#[source] globset::Error),
}

impl GlobError {
    /// The error as the message of a configuration error about the list of globs at `place`, which the caller words as the message shows it, backticks included.
    #[must_use]
    pub fn at(&self, place: &str) -> String {
        match self {
            Self::Invalid { pattern, source } => {
                format!("{place} has the glob {pattern:?}, which is not valid: {source}")
            }
            Self::Build(source) => format!("{place} does not build: {source}"),
        }
    }
}

/// The patterns as one set; an empty list gives a set that matches nothing.
///
/// # Errors
///
/// [`GlobError::Invalid`] names the first pattern that does not parse.
pub fn glob_set(patterns: &[String]) -> Result<GlobSet, GlobError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|source| GlobError::Invalid {
            pattern: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder.build().map_err(GlobError::Build)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(patterns: &[&str]) -> Vec<String> {
        patterns
            .iter()
            .map(|pattern| (*pattern).to_owned())
            .collect()
    }

    #[test]
    fn a_valid_set_matches_its_globs() {
        let set = glob_set(&owned(&["**/migrations/**", "db/*.sql"])).unwrap();
        assert!(set.is_match("prisma/migrations/1_init/migration.sql"));
        assert!(set.is_match("db/schema.sql"));
        assert!(!set.is_match("src/report.sql"));
    }

    #[test]
    fn an_empty_list_matches_nothing() {
        assert!(!glob_set(&[]).unwrap().is_match("migrations/a.sql"));
    }

    #[test]
    fn an_invalid_pattern_is_carried_in_the_error() {
        let error = glob_set(&owned(&["src/**", "["])).unwrap_err();
        assert!(matches!(&error, GlobError::Invalid { pattern, .. } if pattern == "["));
        assert!(error.to_string().starts_with("\"[\" is not a valid glob"));
    }

    #[test]
    fn the_message_names_the_place_and_the_pattern() {
        let invalid = glob_set(&owned(&["["])).unwrap_err();
        let message = invalid.at("`migrations`");
        assert!(message.starts_with("`migrations` has the glob \"[\", which is not valid: "));
    }
}
