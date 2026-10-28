//! `gsttt impact`: what a change affects, for the files named on the command line or the files git lists as changed.

use gsttt_core::Loaded;
use gsttt_core::engine::changes::ChangeKind;
use gsttt_core::query::{self, ImpactRequest};

use super::{Session, print};
use crate::cli::ImpactArgs;
use crate::error::CliError;
use crate::streams::{Io, Outcome};

/// `gsttt impact <path>...` and `gsttt impact --changed`.
pub(super) fn impact(
    session: &Session<'_>,
    args: &ImpactArgs,
    io: &mut Io<'_>,
) -> Result<Outcome, CliError> {
    // Reading the request tells every usage error before the index is read, so a bad command line fails the same
    // way whatever the index holds.
    let request = request(session, args)?;
    let Loaded { index, freshness } = session.loaded()?;
    print::answer(
        &query::impact(&index, freshness, &request),
        session.flags,
        io,
    )
}

/// What to ask: the named files, with those that are not on disk passed as deleted, or the files git lists as changed,
/// where a deleted file is passed as deleted.
fn request(session: &Session<'_>, args: &ImpactArgs) -> Result<ImpactRequest, CliError> {
    let mut request = ImpactRequest {
        depth: args.depth,
        ..ImpactRequest::default()
    };
    if args.changed {
        for change in session.repository.changed(args.base.as_deref())? {
            let list = match change.kind {
                ChangeKind::Deleted => &mut request.deleted,
                ChangeKind::Modified | ChangeKind::Added => &mut request.files,
            };
            list.push(change.path.as_str().to_owned());
        }
    } else {
        for argument in &args.paths {
            let target = session.argument(argument)?;
            let list = if target.on_disk {
                &mut request.files
            } else {
                &mut request.deleted
            };
            list.push(target.path.as_str().to_owned());
        }
    }
    Ok(request)
}
