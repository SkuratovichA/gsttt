# Tasks

## 1. Reading files

- [x] 1.1 `Connector` for `python`: detection, projects, manifests, source roots, options with a JSON schema
- [ ] 1.2 Parsing with ruff: imports of every form, type-checking and optional flags, names, lines, test tags, recovered status
- [ ] 1.3 A `Site` over calls, decorators and assignments (class fields included), and matching of every rule kind

## 2. Resolution

- [x] 2.1 Module paths against source roots, `__init__.py`, stubs and namespace packages; relative imports
- [x] 2.2 Standard library, declared dependencies with normalised names and an import-name table, unknown packages

## 3. Packs and link

- [ ] 3.1 Packs: `fastapi`, `flask`, `django`, `requests`, `httpx`, `sqlalchemy`, `alembic`, `dbapi`, `env`
- [ ] 3.2 `link`: `include_router`, `register_blueprint` and Django `include` across files
- [ ] 3.3 `provides` with blind spots per project

## 4. Proof

- [ ] 4.1 A test annotated `#[scenario("python", …)]` for every scenario of the `python` spec, on fixture trees with reviewed `insta` snapshots
- [ ] 4.2 `cargo nextest run -p gsttt-lang-python`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` pass
