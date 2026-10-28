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
- [x] 5.4 A client or session made with no base URL reads its calls as a module-level call does, so a computed URL with no literal path is an unread site, and a client made where it is called is read like one held in a variable
- [x] 5.5 An embedded document starts on the line its first literal starts on, joined literals keep each part on its own line, and a line break written as an escape adds no line, so a reader of the document puts each fact on the line it is on in the file
- [x] 5.6 An absolute import is looked up in one order, the first place that has the module winning: the project's own source roots, the standard library, the source roots of the other Python projects of the repository, the declared distributions, then namespace directories; resolution goes through the repository view, and a bare namespace import names no file
- [x] 5.7 A package directory beats a module of the same name, and a stub beside its source is reached together with it
- [x] 5.8 An import reaches the `__init__` file of every package it passes through, except the packages the importing file sits in
- [x] 5.9 A name an import gives another is read by both names: `import requests as r`, `from requests import post as send` and the like match the packs' callees, decorators, base classes and subscripted names as the file writes them and as the import spells them, every file keeps its imports for this, and a name imported twice from one module is listed once
- [x] 5.10 A Flask route whose `methods` is held in a variable is read as GET, which the rule language cannot tell from an absent keyword, so the provides of a project with the `flask` pack name it as a blind spot
- [x] 5.11 A `setup.py` list of requirements is read to its own bracket, so a requirement with extras (`fastapi[standard]>=0.1`) no longer ends it, and a comment in the list is skipped
- [x] 5.12 The line of the first error of a file the parser recovered from is a line the file has, also when the error is the bracket left open at the end of the text
- [x] 5.13 The salt of a project is a hash of length-prefixed parts that name the packs its manifests switch on, by position in the list the connector was given, which is all that parsing reads beyond what the key of the engine holds; a failed serialisation no longer gives every project one salt
- [x] 5.14 A call of `execute` or `executemany` leaves an unread site only when its first argument is a string built at run time (an f-string, `+` or `%` on a string, `.format` on a string) or the callee is named outright (`text(query)`), so `pool.execute(task)` records nothing; the provides of every project name receivers of more than three segments and SQL held in a variable as blind spots of its tables
