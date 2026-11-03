# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec deltas

## 2. The pack

- [x] 2.1 `crates/lang-ts/packs/fetch.toml`: `method = "$arg2.method|GET"`

## 3. The tests

- [x] 3.1 The tests and snapshots that spelled `http * <url>` for a `fetch` with no method take `http GET <url>`
- [x] 3.2 `a_fetch_that_writes_no_method_is_a_get` and `a_fetch_whose_options_cannot_be_read_keeps_any_method`, on the fixtures `fetch-no-method` and `fetch-unread-options`
- [x] 3.3 `asking_about_a_get_finds_a_fetch_that_writes_no_method`, over a TypeScript tree through `gsttt contract`
