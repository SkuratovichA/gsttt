# Spec Delta

## Purpose

The Python connector reads Python into imports and contract sites. It parses with ruff's parser, resolves modules against the project's source roots, the standard library and the declared dependencies, and finds HTTP routes and requests, tables, embedded SQL and environment variables through rule packs.

## ADDED Requirements

### Requirement: Projects
The connector SHALL open one project per directory holding a `pyproject.toml`, `setup.py` or `setup.cfg`, and one at the root when the repository has Python files and none of these at the root. A project SHALL claim the `py` and `pyi` files under its root, and its manifests (`pyproject.toml`, `setup.py`, `setup.cfg`, `requirements*.txt`), which it tags as manifests.

#### Scenario: Each pyproject is a project
- **WHEN** a repository has `services/api/pyproject.toml` and `services/worker/pyproject.toml`
- **THEN** the connector opens a project at each, and `services/api/app/main.py` belongs to `services/api`

### Requirement: Source roots
A project's source roots SHALL be the project root, `src` when that directory exists, the directories named by setuptools, Poetry or Hatch package settings, and the roots given in the module's options.

#### Scenario: A src layout is a source root
- **WHEN** a project has `pyproject.toml`, `src/booking/__init__.py` and `src/booking/api.py`, and `tests/test_api.py` imports `booking.api`
- **THEN** the import resolves to `src/booking/api.py` and to the package file `src/booking/__init__.py` it passes through

### Requirement: Imports
The connector SHALL read `import a.b`, `import a.b as c`, `from a.b import c`, relative imports with any number of dots, and `importlib.import_module` and `__import__` with a literal argument, each with its line and names. Imports under `if TYPE_CHECKING:` SHALL be flagged type-checking and imports inside a `try` whose handler catches `ImportError` or `ModuleNotFoundError` SHALL be flagged optional. `importlib.import_module` with a computed argument SHALL be an unread site.

#### Scenario: Every import form is read
- **WHEN** a file holds one import of each form listed above
- **THEN** its facts hold each import with its kind, line and names, matching the reviewed snapshot

#### Scenario: Guarded imports are flagged
- **WHEN** a file imports `a` under `if TYPE_CHECKING:` and `b` in `try: … except ImportError:`
- **THEN** the import of `a` is flagged type-checking and the import of `b` optional

### Requirement: Names of an import
A name imported more than once from one module in a single `from` statement SHALL be listed once among the names of the import.

#### Scenario: A name imported twice is listed once
- **WHEN** a file has `from bookings import views as first, views as second`
- **THEN** its one import of `bookings` lists the name `views` once

### Requirement: Imports under another name
A callee, decorator, base class or subscripted name SHALL be matched by a rule as the file writes it and as its imports spell it, so `import requests as r`, `import httpx as hx`, `import os as _os`, `from requests import post as send`, `from sqlalchemy import text as t` and `from fastapi import APIRouter as R` read like the plain forms. An import SHALL apply from its statement on, and a name bound by a relative import SHALL stay as written.

#### Scenario: A module imported under another name is followed
- **WHEN** a file has `import requests as r` and `r.post(f"{BASE}/bookings/{id}/cancel")` on line 7
- **THEN** it calls `http POST /bookings/{}/cancel` at line 7

#### Scenario: A name imported from a module is followed
- **WHEN** a file has `from requests import get` and `from requests import post as send`, and calls `get(f"{BASE}/bookings/{id}")` and `send(f"{BASE}/bookings/{id}/cancel")`
- **THEN** it calls `http GET /bookings/{}` and `http POST /bookings/{}/cancel`, the method being the one of the function and not of the name it is imported as

#### Scenario: A client made through an aliased module is followed
- **WHEN** a file has `import httpx as hx`, calls `hx.get(f"{BASE}/bookings/{id}")`, and makes `client = hx.Client(base_url="https://api.example.com/v1")` for `client.get("/bookings")`
- **THEN** it calls `http GET /bookings/{}` and `http GET /v1/bookings`

#### Scenario: A function imported under another name embeds its document
- **WHEN** a file has `from sqlalchemy import text as t` and `connection.execute(t("SELECT id FROM bookings WHERE id = :id"), {"id": id})`
- **THEN** it embeds one `sql` document with the text `SELECT id FROM bookings WHERE id = :id`

#### Scenario: A router class imported under another name serves its routes
- **WHEN** a file has `import fastapi as fa`, `from fastapi import APIRouter as R`, `app = fa.FastAPI()`, `router = R(prefix="/bookings")`, `@app.get("/ping")` and `@router.get("/{booking_id}")`
- **THEN** it serves `http GET /ping`, and `http GET /bookings/{}` once the router is linked

#### Scenario: The environment is read through an aliased os
- **WHEN** a file has `import os as _os`, `_os.getenv("A")`, `_os.environ["B"]` and `_os.environ.get("C")`
- **THEN** it calls `env A`, `env B` and `env C`

#### Scenario: Django routes imported under another name compose
- **WHEN** `project/urls.py` has `from django.urls import include as inc` and `path as p` and `p("api/", inc("bookings.urls"))`, and `bookings/urls.py` has `p("bookings/<int:pk>/", views.detail)`
- **THEN** `bookings/urls.py` serves `http * /api/bookings/{}` once linked, and neither file serves a route of its own

### Requirement: Parse status
A file with syntax errors from which the parser recovered SHALL be recorded as recovered with the number of errors and the line of the first, keeping what was read.

#### Scenario: A recoverable error keeps the imports
- **WHEN** a file imports `os` and `app.db` and has an unclosed bracket on its last line
- **THEN** its status is recovered and its facts still hold both imports

### Requirement: Test files
Files named `test_*.py` or `*_test.py`, `conftest.py`, and files under a directory named `tests` or `test` SHALL be tagged as tests.

#### Scenario: Test files are tagged
- **WHEN** the project holds `tests/helpers.py`, `app/test_api.py`, `app/conftest.py` and `app/api.py`
- **THEN** the first three are tagged as tests and `app/api.py` is not

### Requirement: Resolution
A module path SHALL resolve segment by segment under a source root: a package (`a/b/__init__.py`) before a module (`a/b.py`), a stub reached together with the source beside it, and a directory of Python files with no `__init__.py` as a namespace package. `from a import b` SHALL resolve to the module `a.b` when it exists and to `a` otherwise.

#### Scenario: Submodules and names are told apart
- **WHEN** `app/x.py` has `from app import models` and `from app.models import Booking`, and `app/models.py` exists
- **THEN** both imports resolve to `app/models.py`
- **AND** `from app import settings`, where `app/settings.py` does not exist but `app/__init__.py` does, resolves to `app/__init__.py`

#### Scenario: A package beats a module of the same name
- **WHEN** a project holds both `both/__init__.py` and `both.py`, and a file imports `both`
- **THEN** the import resolves to `both/__init__.py` only

#### Scenario: A stub is reached with its source
- **WHEN** a project holds `stubbed.py` and `stubbed.pyi`, and a file imports `stubbed`
- **THEN** the import resolves to both files

### Requirement: Resolution order
An absolute import SHALL look in this order, the first place that has the module winning: the project's own source roots, the standard library, the source roots of the other Python projects of the repository, the distributions the project declares, and last the namespace directories, the project's own before the others'. A module found nowhere SHALL be an unknown package, or not found when its first segment names a directory or package of the repository.

#### Scenario: A library of another project resolves to its files
- **WHEN** `libs/common` and `services/api` are projects, `services/api` declares `common`, and a file of it imports `common.util`, which `libs/common/common/util.py` provides
- **THEN** the import resolves to `libs/common/common/__init__.py` and `libs/common/common/util.py`, not to the package `common`

#### Scenario: A namespace directory does not shadow the standard library or a dependency
- **WHEN** a project declares `PyYAML` and holds `logging/formatters.py`, `yaml/loaders.py` and `ns/part.py`, none of the three directories with an `__init__.py`
- **THEN** `import logging` and `import logging.handlers` resolve to a builtin, `import yaml` to the package `pyyaml`, and `import ns.part` to `ns/part.py`

#### Scenario: A bare namespace import names no file
- **WHEN** a file imports `ns`, a directory with `ns/part.py` and no `__init__.py`, and another imports `ns.missing`
- **THEN** both are not found, because a directory is no file to depend on

### Requirement: Packages on the way
An import SHALL reach the `__init__` file of every package it passes through as well as the module it names, except the packages the importing file sits in, which are initialised before it runs.

#### Scenario: An import reaches the packages it passes through
- **WHEN** `outside.py` has `import pkg.sub.leaf`, `from pkg.sub import leaf` and `from pkg import sub`, and `pkg/__init__.py`, `pkg/sub/__init__.py` and `pkg/sub/leaf.py` exist
- **THEN** the first two resolve to all three files and the third to `pkg/__init__.py` and `pkg/sub/__init__.py`

#### Scenario: An import inside a package adds nothing for its own package
- **WHEN** `pkg/sub/sibling.py` has `import pkg.sub.leaf`, `from .leaf import value` and `from . import leaf`
- **THEN** each resolves to `pkg/sub/leaf.py` only

### Requirement: Relative import resolution
Relative imports SHALL resolve from the importing file's package.

#### Scenario: Relative imports climb packages
- **WHEN** `app/api/routes.py` has `from ..db import session` and `from . import deps`
- **THEN** they resolve to `app/db/session.py` or `app/db.py`, whichever exists, and to `app/api/deps.py`

### Requirement: Standard library and dependencies
A standard library module SHALL resolve to a builtin, a module of a declared dependency to that package (by its normalised distribution name and the known import names of common distributions), and anything else to an unknown package or not found.

#### Scenario: Standard library, dependencies and unknown modules
- **WHEN** a project declares `PyYAML` and `fastapi` and a file imports `json`, `yaml`, `fastapi.routing`, `requests` and `app.missing`
- **THEN** `json` resolves to a builtin, `yaml` to the package `pyyaml`, `fastapi.routing` to the package `fastapi`, `requests` to an unknown package and `app.missing` to not found

### Requirement: HTTP routes
With the `fastapi` pack, `get`, `post`, `put`, `patch`, `delete`, `options`, `head` and `api_route` decorators on a receiver made by `FastAPI()` or `APIRouter()` SHALL serve the path, and `include_router(router, prefix=…)` SHALL serve the included router's routes under the prefix, across files.

#### Scenario: A FastAPI router included with a prefix
- **WHEN** `app/api/bookings.py` has `router = APIRouter(prefix="/bookings")` and `@router.get("/{booking_id}")`, and `app/main.py` has `app.include_router(bookings.router, prefix="/api")`
- **THEN** `app/api/bookings.py` serves `http GET /api/bookings/{}` at the line of the decorator
- **AND** it serves no other path

### Requirement: FastAPI router chains
A route on an `APIRouter` SHALL be served once for each chain of `include_router(router, prefix=…)` calls that reaches the router, at the path made by joining, from the outermost router inwards, the `prefix` of each call and then the `prefix` of the router it includes, and last the path of the route. A router that no call includes SHALL serve its own `prefix` joined with the path.

#### Scenario: A FastAPI router mounted through another router
- **WHEN** `app/api/bookings.py` has `router = APIRouter(prefix="/bookings")` and `@router.get("/{booking_id}")`, `app/api/v1.py` has `sub = APIRouter(prefix="/v1")` and `sub.include_router(bookings.router, prefix="/b")`, and `app/main.py` has `app.include_router(v1.sub, prefix="/api")`
- **THEN** `app/api/bookings.py` serves `http GET /api/v1/b/bookings/{}` and no other path

### Requirement: Repeated and cyclic mounts
A call that includes a router without a `prefix` SHALL add no second copy of its routes, and a chain of calls that comes back to a router it has passed SHALL end there, the routes being served by the chains that end at a router nobody includes.

#### Scenario: A router is served once for each way it is mounted
- **WHEN** `app/main.py` has `app.include_router(health.router)`, `app.include_router(items.router)` and `app.include_router(items.router, prefix="/v2")`, and `@app.get("/ping")`, where `health.router` is `APIRouter()` with `@router.get("/health")` and `items.router` is `APIRouter(prefix="/items")` with `@router.get("/{item_id}")`
- **THEN** `app/health.py` serves `http GET /health` once, `app/items.py` serves `http GET /items/{}` and `http GET /v2/items/{}`, and `app/main.py` serves `http GET /ping`

#### Scenario: Routers that mount each other still serve their routes
- **WHEN** `app/a.py` has `router = APIRouter(prefix="/x")`, `router.include_router(b.router, prefix="/b")` and `@router.get("/one")`, `app/b.py` has `router = APIRouter(prefix="/y")`, `router.include_router(a.router, prefix="/a")` and `@router.get("/two")`, and `app/main.py` has `app.include_router(a.router, prefix="/a")`
- **THEN** `app/a.py` serves `http GET /a/x/one` and `app/b.py` serves `http GET /a/x/b/y/two`

### Requirement: Flask routes
With the `flask` pack, `route` and the verb decorators on a receiver made by `Flask()` or `Blueprint()` SHALL serve the path, the method taken from the first entry of `methods` and `GET` when it is absent, and `register_blueprint(bp, url_prefix=…)` SHALL serve the blueprint's routes under the prefix, across files.

#### Scenario: A Flask blueprint serves under its prefix
- **WHEN** a file has `bp = Blueprint("b", __name__, url_prefix="/bookings")` and `@bp.route("/<int:id>", methods=["DELETE"])`
- **THEN** it serves `http DELETE /bookings/{}`

### Requirement: Flask blueprint prefixes
A route on a `Blueprint` SHALL be served once for each chain of `register_blueprint(bp, url_prefix=…)` calls that reaches the blueprint. A blueprint SHALL be served under the `url_prefix` of its registration, which replaces the one it was made with, and under its own when the registration gives none. A blueprint registered on another blueprint SHALL be served under its parent's prefix, found the same way, followed by its own, and one that no registration reaches under its own.

#### Scenario: A registration prefix replaces a blueprint's own
- **WHEN** `app/bookings.py` has `bp = Blueprint("bookings", __name__, url_prefix="/bookings")` and `@bp.get("/<int:id>")`, `app/reports.py` has `reports = Blueprint("reports", __name__, url_prefix="/reports")` and `@reports.get("/daily")`, and `app/__init__.py` has `app.register_blueprint(bp, url_prefix="/api")` and `app.register_blueprint(reports)`
- **THEN** `app/bookings.py` serves `http GET /api/{}` and not `http GET /bookings/{}`
- **AND** `app/reports.py` serves `http GET /reports/daily`

#### Scenario: Blueprints nest under their parent's prefix
- **WHEN** `admin = Blueprint("admin", __name__, url_prefix="/admin")` registers `reports`, made with `url_prefix="/reports"` and `@reports.get("/daily")`, `shop = Blueprint("shop", __name__, url_prefix="/shop")` registers `items`, made with `url_prefix="/items"` and `@items.get("/<int:id>")`, with `url_prefix="/goods"`, and the app registers `admin` and `shop` with `url_prefix="/store"`
- **THEN** `reports` serves `http GET /admin/reports/daily` and `items` serves `http GET /store/goods/{}`

### Requirement: Django routes
With the `django` pack, `path()` entries in `urlpatterns` SHALL serve their routes, with `include("<module>")` adding the including route as a prefix across files.

#### Scenario: Django includes compose
- **WHEN** `project/urls.py` has `path("api/", include("bookings.urls"))` and `bookings/urls.py` has `path("bookings/<int:pk>/", views.detail)`
- **THEN** `bookings/urls.py` serves `http * /api/bookings/{}` at the line of its `path` entry

### Requirement: HTTP requests
With the `requests` and `httpx` packs, `requests.get` and the other verbs, `requests.request(method, url)`, the same verbs on `httpx` and on receivers made by `requests.Session()`, `httpx.Client()` or `httpx.AsyncClient()`, held in a variable or called where they are made, SHALL call the URL with the verb. An f-string URL SHALL keep its literal parts with each computed part as a parameter, and a URL with no literal path SHALL be an unread site, also when its receiver was made with no base URL.

#### Scenario: Requests become calls
- **WHEN** a file has `requests.post(f"{BASE}/bookings/{id}/cancel")` and `httpx.get(url)`
- **THEN** it calls `http POST /bookings/{}/cancel` and has an unread site at the line of `httpx.get`

#### Scenario: A client with no base URL leaves a computed URL unread
- **WHEN** a file has `session = requests.Session()` and `session.get(f"{base}{path}")`, `httpx.Client().get(f"{url}")`, and `client = httpx.Client(base_url="https://api.example.com/v1")` with `client.get("/bookings")`
- **THEN** it has an unread site at the line of each of the first two calls
- **AND** it calls `http GET /v1/bookings`

### Requirement: Tables and SQL
With the `sqlalchemy` pack, `__tablename__ = "<name>"` in a class and `Table("<name>", …)` SHALL declare `table <name>`, and a string passed to `text()` SHALL be an embedded `sql` document. With the `django` pack, `db_table = "<name>"` in a model's `Meta` SHALL declare `table <name>`.

#### Scenario: Model tables are declared
- **WHEN** one class has `__tablename__ = "bookings"` and a Django model's `Meta` has `db_table = "booking_items"`
- **THEN** the files declare `table bookings` and `table booking_items`

### Requirement: Alembic migrations
With the `alembic` pack, `op.create_table`, `op.drop_table`, `op.rename_table`, `op.add_column`, `op.drop_column`, `op.alter_column` and `op.create_index` SHALL declare `table <name>` for the table they name.

#### Scenario: A migration declares its tables
- **WHEN** an Alembic revision has `op.create_table("bookings", sa.Column("id", sa.Integer))` and `op.add_column("booking_items", sa.Column("note", sa.Text))`
- **THEN** it declares `table bookings` and `table booking_items`

### Requirement: DB-API SQL
With the `dbapi` pack, the first argument of `execute` and `executemany` on any receiver SHALL be an embedded `sql` document when it is a string literal. The document SHALL start on the line its first literal starts on and keep the lines of the source: each part of a literal joined by implicit concatenation or `+` stays on its own line, and a line break written as an escape sequence adds none.

#### Scenario: Raw SQL is embedded
- **WHEN** a file has `cursor.execute("SELECT id FROM bookings WHERE id = %s", (id,))` on line 7
- **THEN** it embeds an `sql` document at line 7 with that text

#### Scenario: A document starts on the line of its first literal
- **WHEN** a file has `cursor.execute(` on line 13 and a triple-quoted literal that opens on line 14, with `SELECT id` on line 15, `FROM bookings` on line 16 and `WHERE id IN %s` on line 17
- **THEN** it embeds an `sql` document at line 14
- **AND** counting its lines from line 14, `FROM bookings` is on line 16

#### Scenario: Joined literals keep the lines of their parts
- **WHEN** a file has `cursor.execute(` followed by the literals `"SELECT id "`, `"FROM bookings "` and `"WHERE id = %s"` on three lines, and another call with the same literals joined by `+`
- **THEN** each call embeds a document at the line of `"SELECT id "`, whose second line is `FROM bookings ` and third is `WHERE id = %s`

#### Scenario: A line break written as an escape adds no line
- **WHEN** a file has `cursor.execute("SELECT id\nFROM bookings\nWHERE id = %s", (id,))` on one line
- **THEN** it embeds an `sql` document on that line, with the text `SELECT id FROM bookings WHERE id = %s`

### Requirement: Environment variables
With the `env` pack, `os.environ["<NAME>"]`, `os.environ.get("<NAME>")`, `os.getenv("<NAME>")` and each field of a class deriving from `BaseSettings` SHALL call `env <NAME>`, a field's name prefixed with the literal `env_prefix` of the class's `model_config` or inner `Config` when it has one.

#### Scenario: Every way of reading the environment is seen
- **WHEN** a file reads `os.environ["A"]`, `os.environ.get("B")`, `os.getenv("C")` and declares `class Settings(BaseSettings): database_url: str`
- **THEN** it calls `env A`, `env B`, `env C` and `env DATABASE_URL`

#### Scenario: A settings prefix is applied
- **WHEN** a class deriving from `BaseSettings` has `model_config = SettingsConfigDict(env_prefix="APP_")` and a field `port: int`
- **THEN** the file calls `env APP_PORT`

### Requirement: Pack activation
A pack SHALL be active in a project when its manifests declare one of the pack's `detect` packages, compared by normalised name, in `[project]` dependencies, optional dependencies, dependency groups, Poetry dependencies or a `requirements*.txt`, or always when its `detect` is empty. Each project SHALL report what it provides, with its blind spots in words.

#### Scenario: Packs follow the manifest
- **WHEN** a project's `pyproject.toml` declares `Flask>=3` and not `fastapi`
- **THEN** Flask routes in it produce sites and FastAPI decorators do not

#### Scenario: Blind spots are reported
- **WHEN** the coverage of a Python project is read
- **THEN** its provides name, among the blind spots, Flask routes with several methods beyond the first, Flask routes whose methods are held in a variable, prefixes held in variables and Django default table names
