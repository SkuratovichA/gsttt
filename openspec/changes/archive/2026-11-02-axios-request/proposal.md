# Proposal: Read axios.request as a call

## Why

`axios.request({ url, method })` and `api.request({ url, method })` on a client made by `axios.create` are the third way to call with axios, after the verbs and `axios(config)`. The `axios` pack reads the other two and says nothing about `request`, so those calls are missing from the graph and leave no unread site to show for them.

## What Changes

- The `axios` pack gets two rules: `axios.request` on the default client, and `*.request` and `*.*.request` on a receiver that `axios.create` makes, under its `baseURL`, as the pack already reads the verbs. Both take the URL from the `url` property of the config and the method from its `method` property, and an absent method reads as `*`.
- A config that is no object literal gives no URL, so the site is an unread site; a template in `url` is read as the other rules read it.
- A new requirement, `Requests through request`, with its scenarios. The requirement `HTTP requests` is not touched.
- No version bump: the content of a pack is part of the module state, so a changed built-in pack parses the files again. `READER_VERSION` and `FACT_FORMAT` stay.

## Impact

- Capability `typescript`.
- `crates/lang-ts/packs/axios.toml`, fixtures, tests and snapshots under `crates/lang-ts/tests/`.
- Calls through `request` appear in the graph where they were missing; a `request` call on a receiver that no `axios.create` makes is still silent, as `get` is.
