# Tasks

## 1. The change

- [x] 1.1 `proposal.md`, `tasks.md` and the spec delta

## 2. The parse

- [x] 2.1 `analyze.rs`: `mount` split into `read_mount` and the two callers, and `imported_mount` for a `.use` on an imported host
- [x] 2.2 `extra.rs`: `ImportedMount` and `TsExtra::imported_mounts`
- [x] 2.3 `project.rs`: `READER_VERSION` one up

## 3. The link

- [x] 3.1 `link.rs`: `parents` walks the imported mounts, for a host the link finds to serve routes, and reports a computed prefix there

## 4. The scenarios

- [x] 4.1 Fixtures `imported-mount-*` and tests for the eight scenarios in `tests/typescript/mounts.rs`, among them the imported client (`imported-mount-client`) and the member of an imported name (`imported-mount-member`)
- [x] 4.2 `provides.rs`: the blind spot of Http Serves, and the snapshots that carry it, and the line of `README.md`
