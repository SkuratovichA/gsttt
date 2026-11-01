# Proposal: Keep a path literal glued to a base URL

## Why

A client written ``axios.get(`${this.url}api/versions/${id}`)`` was keyed `/versions/{}`, because the normaliser dropped the whole first segment as a base URL. It never met the server route `/api/versions/:id` it really calls. A field test on a public microservice codebase showed it.

## What Changes

- A literal glued straight after the placeholder that opens the path, which cannot be part of a host, starts the path: `${base}api/x` becomes `/api/x`. `${HOST}:8080/x`, `${SUB}.example.com/x`, `${user}@host/x`, `${base}-api/x` and `${a}${b}/x` still drop the whole segment. `${scheme}localhost/x` becomes `/localhost/x`, an accepted edge.
- `FACT_FORMAT` goes from 9 to 10, because connectors make keys through the normaliser while they parse and the parsed facts are cached.

## Impact

- Capability `indexing`.
- `crates/core/` only: `normalize_http_path`, `FACT_FORMAT` and tests.
- Indexes built by an earlier version are parsed again once.
