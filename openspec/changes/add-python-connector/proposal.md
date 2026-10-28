# Proposal

## Why

Python services sit on the other side of many TypeScript clients and own many of the tables. Without a Python connector, impact stops at the language border, which is exactly where a change is most likely to break something nobody looked at.

## What Changes

- The `python` connector: projects per `pyproject.toml`, `setup.py` or `setup.cfg`, source roots (`src` layouts and packaging settings), parsing with ruff's parser into imports, flags, tags and sites.
- Resolution against source roots, namespace packages, the standard library (`ruff_python_stdlib`) and declared dependencies with normalised names and known import names.
- Built-in packs in `crates/lang-python/packs/`: `fastapi`, `flask`, `django`, `requests`, `httpx`, `sqlalchemy`, `alembic`, `dbapi`, `env`.
- `link` for FastAPI `include_router`, Flask `register_blueprint` and Django `include`.

## Capabilities

### New Capabilities

- `python`: what the Python connector reads, how it resolves, and which contracts it finds.

### Modified Capabilities

None.

## Impact

`crates/lang-python/` only, plus new entries in the root `[workspace.dependencies]` if needed. Known gaps, reported as blind spots: Flask routes with several methods beyond the first, prefixes and URLs held in variables, Django default table names, GraphQL servers (Strawberry, Graphene), task queues.
