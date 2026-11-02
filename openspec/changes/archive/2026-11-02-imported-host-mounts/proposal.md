# Proposal: Read a mount on an imported host

## Why

`import { app } from './app'; app.use('/api', router)` was not read, so every route of that router was served under the wrong path and a caller of `/api/x` matched nothing. It was left out because a `.use` on an arbitrary import is not a mount: `i18n.use(initReactI18next)` must stay silent, neither a mount nor an unread site. The parse can only record the candidate, and the link decides, because only the link knows what the import leads to.

## What Changes

- TypeScript parse: a `host.use(…)` call whose host is not made in the file but is a name the file imports from a file of its project (the test `imported_receiver` already applies to routes) is kept as an `ImportedMount` in the extra of the file, with the prefixes, the targets, the line, and whether a prefix is not text, with the call as written for that case. The arguments are read by the code that reads a mount on a host made in the file, now split so that both paths share it. The parse reports no unread site on this path.
- TypeScript link: `parents` also walks the imported mounts. `maker_of`, the lookup that reads a call on an imported router, gives the node the name leads to, found through re-exports; the mount is an edge from that node when its file lists it among the hosts it serves routes on, which are the variables made by a call some rule serves routes on, the same decision as for a route on an imported router. A name that leads to no such variable, a member of the name (`server.app.use`) and a name from a package add nothing and stay silent. A prefix that is not text on a host the link finds to serve is an unread site of the file.
- The blind spot of Http Serves that said a mount on an imported host is not read now says that it is read only for the name itself.
- 24 snapshots change only in that blind spot line.
- Versions: `READER_VERSION` of the TypeScript connector goes from 12 to 13, because the extra of a file gains a field and the facts of the same text change.

## Impact

- Capability `typescript`.
- `crates/lang-ts/src/analyze.rs` (`mount`, `imported_mount`, `read_mount`), `extra.rs`, `link.rs` (`parents`), `link/deferred.rs` (`maker_of` visible to the parent), `provides.rs`, `project.rs`, and the line of `README.md` that lists the mount on an imported host.
- A tree with a `use` on an imported app or router gives more routes, each under its mount. A tree with none gives the same index as before.
