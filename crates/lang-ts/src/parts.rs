//! The text an expression reads as when some of its pieces are names that another file declares, which only the link phase can read.

use serde::{Deserialize, Serialize};

use crate::shape::COMPUTED;

/// The most levels of [`Part::Cat`] a part nests. The link data of a file is read back by `serde_json`, which refuses a document nested more than 127 levels deep, and a `Cat` takes three of them (`{"Cat":["All",[…]]}`), so a part that nests this many levels in the deepest place one is kept, a `Shape` under a deferred site, reaches level 106 and reads back whole. A part that nests deeper would make the whole file's link data unreadable and lose every contract of its project, so it is no part: a use reads it as `${}` and a constant is no constant. The test `a_part_at_the_depth_limit_is_read_back` in `shape.rs` pins the number against what `serde_json` allows.
pub(crate) const MAX_DEPTH: usize = 32;

/// The most a part may weigh and the most text a read of one may give. A constant that names another twice doubles what it is built from at every step, in the part and in the text it reads as, so that thirty such constants would ask for gigabytes; this is what stays of them. It is 1 MiB, far more than a path or a base URL, and the weight of a part is the bytes of its texts and names plus one for each piece.
pub(crate) const MAX_COMPOSED: usize = 1 << 20;

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
    /// The pieces `parts` written one after the other. A whole with no name in it needs no link to be read, so it is the text it reads as, or `None` when it reads as none. `None` too for a whole that nests deeper than [`MAX_DEPTH`] or weighs more than [`MAX_COMPOSED`]: a use reads that as `${}` and a constant is no constant.
    pub(crate) fn cat(reads: Reads, mut parts: Vec<Part>) -> Option<Part> {
        if reads == Reads::All && parts.len() == 1 {
            return parts.pop();
        }
        let cat = Part::Cat(reads, parts);
        if cat.has_names() {
            let mut budget = MAX_COMPOSED;
            return cat.fits(&mut budget).then_some(cat);
        }
        cat.lax().map(Part::Text)
    }

    /// Whether the part nests at most [`MAX_DEPTH`] levels of `Cat` and weighs at most `budget`; its weight is taken off `budget`, so that several parts can share one. One walk gives both and stops at the first piece that passes either limit, so a part far past them costs no more to refuse than one at them.
    pub(crate) fn fits(&self, budget: &mut usize) -> bool {
        self.fits_below(0, budget)
    }

    fn fits_below(&self, levels: usize, budget: &mut usize) -> bool {
        let own = match self {
            Self::Text(text) => text.len(),
            Self::Name(name) => name.len(),
            Self::Computed | Self::Cat(..) => 0,
        };
        let Some(left) = budget.checked_sub(own + 1) else {
            return false;
        };
        *budget = left;
        match self {
            Self::Cat(_, parts) => {
                levels < MAX_DEPTH && parts.iter().all(|part| part.fits_below(levels + 1, budget))
            }
            Self::Text(_) | Self::Computed | Self::Name(_) => true,
        }
    }

    /// The names the part is built from, in the order they are written and once for each time they are.
    pub(crate) fn names(&self) -> Vec<&str> {
        let mut names = Vec::new();
        self.collect_names(&mut names);
        names
    }

    fn collect_names<'p>(&'p self, names: &mut Vec<&'p str>) {
        match self {
            Self::Name(name) => names.push(name),
            Self::Cat(_, parts) => parts.iter().for_each(|part| part.collect_names(names)),
            Self::Text(_) | Self::Computed => {}
        }
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
                // The length is summed as the pieces come, so that a whole past the limit is refused before the text of the pieces after it is read, and before the text of the whole is made.
                let mut read = Vec::with_capacity(parts.len());
                let mut length = 0;
                for part in parts {
                    let piece = part.read(resolve);
                    length += piece.as_deref().map_or(COMPUTED.len(), str::len);
                    if length > MAX_COMPOSED {
                        return None;
                    }
                    read.push(piece);
                }
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
    use super::{MAX_COMPOSED, MAX_DEPTH, Part, Reads};

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

    /// `levels` pieces of `Cat`, one inside the other, with a name at the bottom.
    fn nested(levels: usize) -> Option<Part> {
        let mut part = name("BASE");
        for _ in 0..levels {
            part = Part::cat(Reads::Always, vec![part, text("/x")])?;
        }
        Some(part)
    }

    #[test]
    fn a_part_nests_as_deep_as_the_limit_and_no_deeper() {
        assert!(nested(MAX_DEPTH).is_some());
        assert_eq!(nested(MAX_DEPTH + 1), None);
        assert_eq!(nested(MAX_DEPTH * 100), None);
    }

    #[test]
    fn a_part_weighs_as_much_as_the_limit_and_no_more() {
        // The cat itself weighs one, the name two, and the text one more than its bytes.
        let fills = MAX_COMPOSED - 1 - 2 - 1;
        let at_the_limit = Part::cat(Reads::Always, vec![name("N"), text(&"x".repeat(fills))]);
        assert!(at_the_limit.is_some());
        let past_it = Part::cat(Reads::Always, vec![name("N"), text(&"x".repeat(fills + 1))]);
        assert_eq!(past_it, None);
    }

    #[test]
    fn a_part_that_is_refused_for_its_weight_is_no_text_in_a_constant_and_in_a_use() {
        let heavy = text(&"x".repeat(MAX_COMPOSED));
        assert_eq!(Part::cat(Reads::All, vec![name("N"), heavy.clone()]), None);
        assert_eq!(Part::cat(Reads::Any, vec![name("N"), heavy]), None);
    }

    #[test]
    fn a_read_gives_as_much_text_as_the_limit_and_no_more() {
        let both = cat(Reads::Always, vec![name("A"), name("B")]);
        let of = |length: usize| move |_: &str| Some("x".repeat(length));
        let half = MAX_COMPOSED / 2;
        assert_eq!(
            both.read(&of(half)).map(|text| text.len()),
            Some(MAX_COMPOSED)
        );
        assert_eq!(both.read(&of(half + 1)), None);
    }

    #[test]
    fn a_read_past_the_limit_is_none_whatever_the_pieces_must_read() {
        let big = |_: &str| Some("x".repeat(MAX_COMPOSED));
        for reads in [Reads::Always, Reads::Any, Reads::All] {
            let both = cat(reads, vec![name("A"), name("B")]);
            assert_eq!(both.read(&big), None, "{reads:?}");
        }
    }

    #[test]
    fn a_read_counts_the_pieces_that_read_as_no_text_as_computed() {
        let none = |_: &str| None;
        let computed = Part::Cat(Reads::Always, vec![Part::Computed; MAX_COMPOSED / 3 + 1]);
        assert_eq!(computed.read(&none), None);
    }

    #[test]
    fn the_names_of_a_part_come_in_the_order_they_are_written() {
        let inner = cat(Reads::All, vec![name("B"), text("/"), name("C")]);
        let part = cat(Reads::Always, vec![name("A"), inner, name("A")]);
        assert_eq!(part.names(), ["A", "B", "C", "A"]);
        assert_eq!(text("/a").names(), Vec::<&str>::new());
    }
}
