//! Reads one Python file into facts.

use gsttt_core::connector::{Parsed, SourceFile};
use gsttt_core::fact::FileFacts;

use crate::extra::PyExtra;
use crate::files::tags_of;

/// Tags the file; what it imports and declares is added once the parser is wired in.
pub(crate) fn parse(file: &SourceFile<'_>) -> Parsed<PyExtra> {
    Parsed::new(FileFacts {
        tags: tags_of(file.path),
        ..FileFacts::default()
    })
}
