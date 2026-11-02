//! The text an expression reads as when some of its pieces are names that another file declares, which only the link phase can read.

use serde::{Deserialize, Serialize};

use crate::shape::COMPUTED;

/// A piece of text, or pieces written one after the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Part {
    /// Text that is known.
    Text(String),
    /// A piece that is no text: it reads as `${}` between pieces that are, and as no text where text is needed.
    Computed,
    /// A name that starts at an import of the file, to be read by the file it comes from.
    Name(String),
    /// Pieces written one after the other, and what they must read for the whole to read.
    Cat(Reads, Vec<Part>),
}

/// What the pieces of a [`Part::Cat`] must read for the whole to read as text. A piece that does not read is written `${}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Reads {
    /// A template is text whatever its pieces read.
    Always,
    /// A concatenation `a + b` is text when one of its operands is.
    Any,
    /// A constant is text only when everything it is built from is.
    All,
}

/// Why an expression is read, which decides what it does with a piece it cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A use of the expression, where a rule reads text: a piece that reads as no text is `${}` between pieces that are, and a URL or an array may give the text.
    Use,
    /// The initializer of a constant: it is text only when every piece of it reads, so a piece that does not read leaves it no constant.
    Declare,
}

impl Mode {
    /// What the pieces of a template must read.
    pub(crate) fn template(self) -> Reads {
        match self {
            Self::Use => Reads::Always,
            Self::Declare => Reads::All,
        }
    }

    /// What the operands of `a + b` must read.
    pub(crate) fn sum(self) -> Reads {
        match self {
            Self::Use => Reads::Any,
            Self::Declare => Reads::All,
        }
    }
}

impl Part {
    /// The pieces `parts` written one after the other. A whole with no name in it needs no link to be read, so it is the text it reads as, or `None` when it reads as none.
    pub(crate) fn cat(reads: Reads, mut parts: Vec<Part>) -> Option<Part> {
        if reads == Reads::All && parts.len() == 1 {
            return parts.pop();
        }
        let cat = Part::Cat(reads, parts);
        if cat.has_names() {
            return Some(cat);
        }
        cat.lax().map(Part::Text)
    }

    /// Whether the part holds a name that another file declares.
    pub(crate) fn has_names(&self) -> bool {
        match self {
            Self::Name(_) => true,
            Self::Cat(_, parts) => parts.iter().any(Self::has_names),
            Self::Text(_) | Self::Computed => false,
        }
    }

    /// The text the part reads as, given what `resolve` finds for each name. `None` when it reads as no text.
    pub(crate) fn read(&self, resolve: &impl Fn(&str) -> Option<String>) -> Option<String> {
        match self {
            Self::Text(text) => Some(text.clone()),
            Self::Computed => None,
            Self::Name(name) => resolve(name),
            Self::Cat(reads, parts) => {
                let read: Vec<Option<String>> =
                    parts.iter().map(|part| part.read(resolve)).collect();
                let enough = match reads {
                    Reads::Always => true,
                    Reads::Any => read.iter().any(Option::is_some),
                    Reads::All => read.iter().all(Option::is_some),
                };
                enough.then(|| {
                    read.iter()
                        .map(|text| text.as_deref().unwrap_or(COMPUTED))
                        .collect()
                })
            }
        }
    }

    /// The text the part reads as when no name reads: what the parse of a file knows.
    pub(crate) fn lax(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            other => other.read(&|_| None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Part, Reads};

    fn text(text: &str) -> Part {
        Part::Text(text.to_owned())
    }

    fn name(name: &str) -> Part {
        Part::Name(name.to_owned())
    }

    /// Reads `BASE` as `/api` and every other name as nothing.
    fn base(name: &str) -> Option<String> {
        (name == "BASE").then(|| "/api".to_owned())
    }

    #[test]
    fn a_whole_with_no_name_is_the_text_it_reads_as() {
        assert_eq!(
            Part::cat(Reads::Always, vec![text("/a/"), Part::Computed, text("/b")]),
            Some(text("/a/${}/b"))
        );
        assert_eq!(
            Part::cat(Reads::Any, vec![Part::Computed, text("/b")]),
            Some(text("${}/b"))
        );
        assert_eq!(
            Part::cat(Reads::All, vec![text("/a"), text("/b")]),
            Some(text("/a/b"))
        );
    }

    #[test]
    fn a_concatenation_of_pieces_that_are_no_text_is_no_text() {
        assert_eq!(
            Part::cat(Reads::Any, vec![Part::Computed, Part::Computed]),
            None
        );
    }

    #[test]
    fn a_constant_of_one_piece_is_that_piece() {
        assert_eq!(
            Part::cat(Reads::All, vec![name("BASE")]),
            Some(name("BASE"))
        );
    }

    /// The part that `Part::cat` makes of pieces that hold a name.
    fn cat(reads: Reads, parts: Vec<Part>) -> Part {
        Part::cat(reads, parts).unwrap()
    }

    #[test]
    fn a_template_reads_as_text_whatever_its_names_read() {
        let template = cat(Reads::Always, vec![name("BASE"), text("/users")]);
        assert_eq!(template.read(&base), Some("/api/users".to_owned()));
        assert_eq!(template.read(&|_| None), Some("${}/users".to_owned()));
    }

    #[test]
    fn a_concatenation_reads_when_one_operand_does() {
        let sum = cat(Reads::Any, vec![name("BASE"), name("OTHER")]);
        assert_eq!(sum.read(&base), Some("/api${}".to_owned()));
        assert_eq!(sum.read(&|_| None), None);
    }

    #[test]
    fn a_constant_reads_only_when_everything_it_is_built_from_does() {
        let constant = cat(Reads::All, vec![name("BASE"), text("/"), name("OTHER")]);
        assert_eq!(constant.read(&base), None);
        let both = |name: &str| Some(format!("<{name}>"));
        assert_eq!(constant.read(&both), Some("<BASE>/<OTHER>".to_owned()));
    }

    #[test]
    fn a_constant_inside_a_template_is_all_or_nothing() {
        let constant = cat(Reads::All, vec![name("BASE"), text("/"), name("OTHER")]);
        let template = cat(Reads::Always, vec![constant, text("/x")]);
        assert_eq!(template.read(&base), Some("${}/x".to_owned()));
    }

    #[test]
    fn lax_reads_every_name_as_nothing() {
        assert_eq!(name("BASE").lax(), None);
        assert_eq!(text("/a").lax(), Some("/a".to_owned()));
        assert_eq!(
            Part::Cat(Reads::Always, vec![name("BASE"), text("/a")]).lax(),
            Some("${}/a".to_owned())
        );
    }
}
