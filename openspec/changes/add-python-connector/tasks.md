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
- [x] 5.15 An assignment rule matches a bare name in a class body also as qualified by the classes around it, so the `django` pack's `*.Meta.db_table` reads a `db_table` only in a `Meta` inside a class and the `sqlalchemy` pack's `*.__tablename__` a `__tablename__` only in a class body, while a target of one segment still matches wherever the name is assigned
- [x] 5.16 The `env` pack reads `os.environ.setdefault("X", …)` and `environ.setdefault("X", …)` as a read of `X`
- [x] 5.17 The `env` pack names a settings field after a literal `validation_alias` or `alias` of the call it is assigned, with no prefix in front, and an alias that is not a literal leaves an unread site in place of the variable named after the field
- [x] 5.18 The provides name the fields of a settings class that derives from another settings class, and an alias written inside a field's annotation, as blind spots of the `env` pack
- [x] 5.19 A `pyproject.toml` that is not valid TOML is a failed file in the coverage, a syntax failure with the line and the message of the TOML parser, and its project still opens
- [x] 5.20 The project at the repository root opens only when some Python file lies under no directory with a manifest, in place of whenever the root has no manifest of its own
- [x] 5.21 A requirements file is read together with the files it includes with `-r`, `-rFILE`, `--requirement FILE` and `--requirement=FILE`, each relative to the file that names it and read once, an include outside the repository is skipped and `-c` is not followed, and a comment starts at a `#` after any whitespace
- [x] 5.22 The text files of a `requirements` directory are claimed as manifests of the project that holds them, like `requirements*.txt`
- [x] 5.23 A fixture and a scenario pin the relative imports that climb above the project root, into a sibling project and out of the repository, which the tests left open
- [x] 5.24 `importlib.import_module(".m", package="p")` with literals imports `p.m`, taken from the package with the rules of `importlib`; a relative name with no package or with `__package__` is taken from the importing file, and a package that is not a literal or that the name climbs out of makes the call an unread site

## 6. Review fixes, round 3

- [x] 6.1 A project of the repository is searched for modules only when the project declares it, a dependency naming the distribution the other project's manifest gives it, in place of every project of the repository; the name is read from `[project]`, `[tool.poetry]`, `setup.cfg` and a literal `name=` of `setup.py`
- [x] 6.2 A regular package or module that has the first segment of an import and not the rest ends the search of the source roots, so a package of the same name in a later root, the project's or another's, supplies nothing
- [x] 6.3 The documents of the sibling roots and of the resolution order say which projects are searched and in what order
- [x] 6.4 A mount that names a router through a package that re-exports it, `from .bookings import router as bookings_router` in the package's `__init__.py`, reaches the router of the module the import names, in place of a router of the package that does not exist; the imports followed are bounded
- [x] 6.5 A router whose every way up through the mounts is cut, by a cycle, by the limit on depth or on ways, or by a prefix that is not a literal, serves no route, in place of one at its own prefix that nothing serves; a router that no mount reaches is still served at its own
- [x] 6.6 A mount whose prefix is a variable, `include_router(r, prefix=PREFIX)`, `register_blueprint(bp, url_prefix=URL)` or `path(PREFIX, include(…))`, is a cut way and not a mount with no prefix, and the call stays an unread site
- [x] 6.7 The provides name every way a chain of mounts is cut in one blind spot, the mount of another project, a cycle, a chain deeper than 32 routers, more than 64 ways, a prefix held in a variable, an import that cannot be resolved and a re-export outside a package's `__init__.py`, and the FastAPI pack names applications mounted with `mount` in another
- [x] 6.8 A name written with dots, `self.__tablename__` or `Model.Meta.db_table`, sets an attribute of an object and declares no table, because the qualified targets of the packs name the classes around a plain assignment
- [x] 6.9 An `env_prefix` that is not a literal, in a `SettingsConfigDict` call, a `model_config` dict or an inner `Config`, leaves the fields that would take their name from it as unread sites, and an aliased field is still read
- [x] 6.10 A pattern that starts with `*` meets a name as written only, so a function imported by name from a library is no `*.execute` or `*.get`, and the env pack names the two libraries that define `BaseSettings` for a class that imports it under another name
- [x] 6.11 A name that two imports bind stands for the second from its statement on, each call going with the import above it, and the provides of the client packs name a client library imported under one name in `try` and in `except`
- [x] 6.12 A `setup.py` keyword is read only when its bracket follows the `=` directly, so a name or a call as the value and the bracket of a later keyword give nothing, and a backslash keeps the next character in its string
