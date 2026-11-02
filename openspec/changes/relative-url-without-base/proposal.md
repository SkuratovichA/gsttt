# Proposal: Say what a relative request with no base URL means

## Why

A request whose target is relative and has no base is silent: it is neither a fact nor an unread site, so coverage cannot say what it missed. In TypeScript `axios.create().get('users/me')` calls `GET /users/me`, while `axios.get('users')` and `fetch('items')` are silent. In Python `requests.get("users")`, `requests.Session().get("users/me")` and `httpx.Client().get("items")` are silent, and so is `reqwest::get("users")` in Rust. In a browser a relative URL resolves against the page, so a rooted path is a fair reading for TypeScript. In Python and Rust such a call fails at run time, so an unread site is the honest reading.

## What Changes

- Core: a pack gets `relative_without_base`, `rooted` or `unread`, left out of its serialisation when unset. A request rule with a `receiver` reads as `rooted` when its pack sets none, and a rule with no receiver is no request, as before.
- Core: `CallRule::behind` tells the three bases apart: no base or an empty one is `None`, text is the prefix, and a base that is there and unread is an error under a rule that serves routes and an empty prefix under any other. `CallRule::apply` takes the setting of the pack, and the TypeScript, Python and Rust connectors pass what `behind` gave straight on, in place of the filters each had of its own.
- Core: a relative target with no base is decided in one place. A leading `./` goes first. Then an empty target, `.`, one that starts with `?` or `#`, and one with a scheme before its first `/`, `?` or `#` (`data:x`, `localhost:3000/x`, but not `12:30/x`) name no path and are no request. A target with a `..` segment, and one that would read as a URL only because a `./` went (``./${x}/y``), is an unread site. Otherwise the target without its leading `./` is the path under `rooted`, and the site is unread under `unread`. A base that is there and unread is decided the same way, as `rooted`.
- Core: the unread site of a relative request with no readable base says `unsupported: relative URL with no readable base`, since "computed at run time" is false for a literal.
- Rust: a type that a `use` of the file brings in from another crate (`use memcache::Client;`) is not a client of a pack, so `self.cache.get("users")` on it is no site. A `use` from this crate may re-export one and is read as written.
- Python: a call rule that serves routes, on a receiver whose base is unread, is now an unread site where it gave a route with no prefix.
- Core: `has_parent_segment` joins `without_query_and_fragment` in `contract.rs`, and `url_join` of the TypeScript connector uses it for the `..` check it had of its own.
- Packs: `axios` and `fetch` are `rooted`, `requests`, `httpx` and `reqwest` are `unread`. A project pack may set the field.
- A client of Python or Rust whose base is there and cannot be read now calls the path it names, as `axios.create({ baseURL: X }).get('users')` already did in TypeScript: `httpx.Client(base_url=settings.API).get("items")` calls `GET /items`.
- Versions: `FACT_FORMAT` goes from 13 to 14, because the third case above and a project pack's receiver rule in Python and Rust give another outcome for the same text under a pack that hashes as before. `LAYOUT` stays, since no archived type changes shape. The connectors keep their versions: what they read from a file is the same, and `FACT_FORMAT` drops the cached facts.

## Limitations

- A local function named `fetch` is read as the global one, as it already is for an absolute path.
- A page that is nested under the root resolves a relative URL against its own path, which the `rooted` reading does not see.

## Impact

- Capabilities `indexing`, `config`, `typescript`, `python` and `rust`.
- `crates/core/src/rules.rs`, `crates/core/src/contract.rs`, `FACT_FORMAT` in `crates/core/src/engine/keys.rs`, the call sites of the three connectors, five built-in packs and the pinned pack schema.
- A tree with no relative request, and no client of Python or Rust with a base that is not text, gives the same index as before. Python and Rust gain unread sites where a relative request had none, and TypeScript gains calls to the rooted path.
