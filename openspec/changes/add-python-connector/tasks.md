# Tasks

## 1. Reading files

- [x] 1.1 `Connector` for `python`: detection, projects, manifests, source roots, options with a JSON schema
- [x] 1.2 Parsing with ruff: imports of every form, type-checking and optional flags, names, lines, test tags, recovered status
- [x] 1.3 A `Site` over calls, decorators and assignments (class fields included), and matching of every rule kind

## 2. Resolution

- [x] 2.1 Module paths against source roots, `__init__.py`, stubs and namespace packages; relative imports
- [x] 2.2 Standard library, declared dependencies with normalised names and an import-name table, unknown packages

## 3. Packs and link

- [x] 3.1 Packs: `fastapi`, `flask`, `django`, `requests`, `httpx`, `sqlalchemy`, `alembic`, `dbapi`, `env`
- [x] 3.2 `link`: `include_router`, `register_blueprint` and Django `include` across files
- [x] 3.3 `provides` with blind spots per project

## 4. Proof

- [x] 4.1 A test annotated `#[scenario("python", …)]` for every scenario of the `python` spec, on fixture trees with reviewed `insta` snapshots
- [x] 4.2 `cargo nextest run -p gsttt-lang-python`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass

## 5. Review fixes

- [x] 5.1 Mounts compose along the whole chain: a FastAPI route is served behind the prefix of every call and router on the way up, a Flask registration prefix replaces the blueprint's own, nested blueprints stack, and a mount called on a router imported from another file continues from that router
- [x] 5.2 A route on a router is served once for each way the router is mounted and never also under the bare prefix; a mount without a prefix adds no second copy
- [x] 5.3 `link` reads the ways up from each router once, ends a chain that comes back to a router it passed, caps the ways at 64 and the depth at 32, and names what it cut in the log
