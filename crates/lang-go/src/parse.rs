//! One Go file into facts.

use gsttt_core::connector::{Parsed, SourceFile};
use gsttt_core::fact::{FileFacts, ParseFailure, ParseStatus, Symbols};

use crate::analyze::analyze;
use crate::file::FileCx;
use crate::files::{is_manifest, tags_of};
use crate::project::GoProject;
use crate::syntax::{parse_tree, scan, top_level_intact};

/// Reads `file`. A `go.mod` is a manifest the project read when it opened, so it carries a tag and nothing else.
pub(crate) fn parse(project: &GoProject, file: &SourceFile<'_>) -> Parsed<()> {
    let tags = tags_of(file.path);
    if is_manifest(file.path) {
        return Parsed::new(FileFacts {
            tags,
            ..FileFacts::default()
        });
    }
    let Some(tree) = parse_tree(file.text) else {
        return Parsed::new(FileFacts::failed(ParseFailure::Unreadable {
            message: format!("{}: the Go parser gave no tree", file.path),
        }));
    };
    let root = tree.root_node();
    let found = scan(root);
    let mut errors = found.errors;
    if let Some(line) = found.too_deep {
        // Nothing below this nesting is read, so the file keeps the line where it went too deep as its first error.
        errors.note(line);
        return Parsed::new(FileFacts {
            status: errors.status(),
            tags,
            ..FileFacts::default()
        });
    }
    let cx = FileCx::build(file.text, root, &project.packs, &project.modules);
    let analysis = analyze(&cx, root);
    if analysis.cut {
        errors.note(1);
    }
    let status = errors.status();
    // A recovery stays inside one function, so the declarations of a file whose top level is intact are all there. A damaged top level may hide any declaration, so that file stays opaque.
    let clean = match status {
        ParseStatus::Ok => true,
        ParseStatus::Recovered { .. } => !analysis.cut && top_level_intact(root),
        ParseStatus::Failed(_) => false,
    };
    let (symbols, exports) = if clean {
        let exports = analysis.exports;
        (
            Some(Symbols {
                declared: analysis.declared,
                on_load: analysis.on_load,
                exports: Vec::new(),
                heirs: analysis.heirs,
                overrides: Vec::new(),
                reads: analysis.reads,
            }),
            Some(exports),
        )
    } else {
        (None, None)
    };
    Parsed::new(FileFacts {
        status,
        tags,
        imports: analysis.imports,
        contracts: analysis.contracts,
        embedded: analysis.embedded,
        unresolved: analysis.unresolved,
        exports,
        package: clean.then_some(analysis.package),
        symbols,
    })
}
