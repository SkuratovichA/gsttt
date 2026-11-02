# python Specification

## Purpose
The Python connector reads Python into imports and contract sites. It parses with ruff's parser, resolves modules against the project's source roots, the standard library and the declared dependencies, and finds HTTP routes and requests, tables, embedded SQL and environment variables through rule packs.

## Requirements

### Requirement: Projects
The connector SHALL open one project per directory holding a `pyproject.toml`, `setup.py` or `setup.cfg`, and one at the root when a Python file lies under none of those directories. A project SHALL claim the `py` and `pyi` files under its root, and its manifests (`pyproject.toml`, `setup.py`, `setup.cfg`, `requirements*.txt` and the text files of a `requirements` directory), which it tags as manifests.

#### Scenario: Each pyproject is a project
- **WHEN** a repository has `services/api/pyproject.toml` and `services/worker/pyproject.toml`
- **THEN** the connector opens a project at each, and `services/api/app/main.py` belongs to `services/api`

#### Scenario: The root project opens only for Python files outside every project
- **WHEN** every Python file of a repository lies under a directory with a `pyproject.toml`, and when another repository also has `tools/release.py` under none of them
- **THEN** the first repository has no project at the root, and the second has one that owns `tools/release.py`

### Requirement: Malformed manifests
A `pyproject.toml` that is not valid TOML SHALL be a failed file in the coverage, a syntax failure with the line and the message of the TOML parser, so a project that lost the packs and source roots the manifest would have given is not silent about it. The project SHALL still open and read its Python files.

#### Scenario: A malformed pyproject.toml is a failed file
- **WHEN** a project's `pyproject.toml` has a string left open on line 2, and another has an array left open on its last line
- **THEN** the first is a failed file with a syntax failure on line 2 and the second one on line 3, each with the message of the parser, and the Python files of the first are read

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
A callee, decorator, base class or subscripted name SHALL be matched by a rule as the file writes it and as its imports spell it, so `import requests as r`, `from requests import post as send` and `from fastapi import APIRouter as R` read like the plain forms, except by a pattern that starts with `*`, which SHALL meet the name as written only. An import SHALL apply from its statement on, and a name bound by a relative import SHALL stay as written.

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

#### Scenario: A function imported by name is no method of a receiver
- **WHEN** a file has `from mylib import execute, get` and `from pydantic_settings import BaseSettings as Base`, calls `execute("rm -rf /tmp/x")`, `execute(f"SELECT * FROM {table}")`, `get("https://example.com/x")` and `cursor.execute("SELECT 1 FROM bookings")`, and has `class Settings(Base)` with the field `port: int`
- **THEN** it embeds the one `sql` document of the `cursor.execute` call, leaves no unread site and calls `env PORT`

#### Scenario: Django routes imported under another name compose
- **WHEN** `project/urls.py` has `from django.urls import include as inc` and `path as p` and `p("api/", inc("bookings.urls"))`, and `bookings/urls.py` has `p("bookings/<int:pk>/", views.detail)`
- **THEN** `bookings/urls.py` serves `http * /api/bookings/{}` once linked, and neither file serves a route of its own

### Requirement: Names bound by two imports
A name that two imports bind one after the other SHALL stand, from the second on, for the second, the file being read as one scope: each call goes with the import just above it, and a fallback import in `try` and `except` is read as the `except` one. The provides of a project with the `requests` or `httpx` pack SHALL name a client library imported under one name in `try` and in `except` as a blind spot.

#### Scenario: A name bound by two imports is read as the last
- **WHEN** a project declares `requests` and `httpx`, and a file has `import requests as c` and `c.get("/x")` in one function, `import httpx as c` and `c.get("/y")` in the next, then `import httpx as http` in a `try`, `import requests as http` in its `except` and `http.get("/z")`
- **THEN** the calls go with `requests`, `httpx` and `requests`, and the provides name a client library imported under one name in `try` and in `except` as a blind spot

### Requirement: Names from a star import
A name that `from <module> import *` brings in SHALL not be matched to the module, because the names it binds are not listed, and the provides of a project SHALL name such names as a blind spot of the contracts that the packs in force find by the imports of a file.

#### Scenario: A function from a star import is not followed
- **WHEN** a project with the `requests` pack has a file with `from requests import *` that calls `get(f"{BASE}/bookings/{id}")`
- **THEN** the file calls no contract
- **AND** the provides of the project name names brought in by a star import as a blind spot of the HTTP calls

### Requirement: Parse status
A file with syntax errors from which the parser recovered SHALL be recorded as recovered with the number of errors and the line of the first, keeping what was read.

#### Scenario: A recoverable error keeps the imports
- **WHEN** a file imports `os` and `app.db` and has an unclosed bracket on its last line, the line break after it being the end of the file
- **THEN** its status is recovered with the last line as the line of the first error, and its facts still hold both imports

### Requirement: Test files
Files named `test_*.py` or `*_test.py`, `conftest.py`, and files under a directory named `tests` or `test` SHALL be tagged as tests.

#### Scenario: Test files are tagged
- **WHEN** the project holds `tests/helpers.py`, `app/test_api.py`, `app/conftest.py` and `app/api.py`
- **THEN** the first three are tagged as tests and `app/api.py` is not

### Requirement: Resolution
A module path SHALL resolve segment by segment under a source root: a package (`a/b/__init__.py`) before a module (`a/b.py`), a stub reached together with the source beside it, and a directory of Python files with no `__init__.py` as a namespace package. Files that are all stubs SHALL lose to a distribution the project declares for the module. `from a import b` SHALL resolve to the module `a.b` when it exists and to `a` otherwise.

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

#### Scenario: A stub without its source loses to a declared distribution
- **WHEN** a project declares `foo`, `bar`, `real` and `mixed`, and holds `foo.pyi`, `bar/__init__.pyi`, `bar/sub.pyi`, `baz.pyi`, `real.py`, `real.pyi`, `mixed/__init__.py` and `mixed/only.pyi`
- **THEN** `import foo` and `import bar.sub` resolve to the packages `foo` and `bar`, and `import baz` resolves to `baz.pyi`
- **AND** `import real` resolves to `real.py` and `real.pyi`, and `import mixed.only` to `mixed/__init__.py` and `mixed/only.pyi`

### Requirement: Resolution order
An absolute import SHALL look in this order, the first place that has the module winning: the project's own source roots, the standard library, the source roots of the projects of the repository that the project declares, the distributions the project declares, and last the namespace directories, the project's own before the others'. A module found nowhere SHALL be an unknown package, or not found when its first segment names a directory or package of the repository.

#### Scenario: A library of another project resolves to its files
- **WHEN** `libs/common` and `services/api` are projects, `services/api` declares `common`, and a file of it imports `common.util`, which `libs/common/common/util.py` provides
- **THEN** the import resolves to `libs/common/common/__init__.py` and `libs/common/common/util.py`, not to the package `common`

#### Scenario: A project of the repository is searched only when it is declared
- **WHEN** `services/api` declares `requests` and `common`, `libs/common` is a project named `common`, `tools` is a project named `tools` with `tools/requests/__init__.py` and `tools/helper/__init__.py`, and a file of `services/api` imports `requests`, `common.util` and `helper`
- **THEN** `requests` resolves to the package `requests`, `common.util` to the files of `libs/common`, and `helper` to an unknown package

#### Scenario: A namespace directory does not shadow the standard library or a dependency
- **WHEN** a project declares `PyYAML` and holds `logging/formatters.py`, `yaml/loaders.py` and `ns/part.py`, none of the three directories with an `__init__.py`
- **THEN** `import logging` and `import logging.handlers` resolve to a builtin, `import yaml` to the package `pyyaml`, and `import ns.part` to `ns/part.py`

#### Scenario: A bare namespace import names no file
- **WHEN** a file imports `ns`, a directory with `ns/part.py` and no `__init__.py`, and another imports `ns.missing`
- **THEN** both are not found, because a directory is no file to depend on

### Requirement: A package hides its name further along
A regular package or module of a source root that has the first segment of an import and not the rest SHALL end the search of the source roots, so a package of the same name in a root after it supplies nothing and the import is not found.

#### Scenario: A regular package hides the same name further along
- **WHEN** `a` holds `a/util/__init__.py` and declares the project `b`, which holds `b/util/__init__.py` and `b/util/two.py`, and a file of `a` imports `util.two` and `util`
- **THEN** `util.two` is not found and `util` resolves to `a/util/__init__.py`

### Requirement: Packages on the way
An import SHALL reach the `__init__` file of every package it passes through as well as the module it names, except the packages the importing file sits in, which are initialised before it runs.

#### Scenario: An import reaches the packages it passes through
- **WHEN** `outside.py` has `import pkg.sub.leaf`, `from pkg.sub import leaf` and `from pkg import sub`, and `pkg/__init__.py`, `pkg/sub/__init__.py` and `pkg/sub/leaf.py` exist
- **THEN** the first two resolve to all three files and the third to `pkg/__init__.py` and `pkg/sub/__init__.py`

#### Scenario: An import inside a package adds nothing for its own package
- **WHEN** `pkg/sub/sibling.py` has `import pkg.sub.leaf`, `from .leaf import value` and `from . import leaf`
- **THEN** each resolves to `pkg/sub/leaf.py` only

### Requirement: Calls that import by name
A call SHALL be read as `importlib.import_module` only when the file's imports make its function that one, whatever it is called there, so a function named `import_module` that the file defines or takes from another module SHALL NOT be read as an import. `importlib.import_module` and `__import__` called with the empty string SHALL import nothing and leave no site, since `importlib` rejects the name.

#### Scenario: A function named import_module that is not importlib's is not an import
- **WHEN** a file defines `import_module(path)` and calls `import_module("plugins.csv")`, and another file has `from loader import import_module` and calls `import_module("plugins.json")`
- **THEN** neither call is an import or an unread site

#### Scenario: A dynamic import of the empty name imports nothing
- **WHEN** a file calls `importlib.import_module("")` and `__import__("")`
- **THEN** neither call is an import or an unread site

### Requirement: Dynamic imports with a package
`importlib.import_module` with a relative literal name SHALL import the name taken from its literal `package`, given as the second argument or as a keyword, one component less out of the package than the name has dots. With no package, or with `__package__`, a relative name SHALL be taken from the package of the importing file. A package that is not a literal, or that the name climbs out of, SHALL make the call an unread site.

#### Scenario: A relative dynamic import is taken from its package
- **WHEN** a file calls `importlib.import_module(".m", package="p")`, `import_module("..m", "p.q")`, `import_module(".n", package="p.q")` and `import_module(name=".m", package="p")`
- **THEN** they import `p.m`, `p.m`, `p.q.n` and `p.m`

#### Scenario: A relative dynamic import with no package is taken from the file's
- **WHEN** a file calls `importlib.import_module(".local", __package__)`, `importlib.import_module(".local")` and `importlib.import_module("p.m", package="other")`
- **THEN** the first two import `.local`, which resolves beside the file, and the last imports `p.m`, since a name with no leading dot has no use for a package

#### Scenario: A dynamic import with a package that cannot be read is an unread site
- **WHEN** a file calls `importlib.import_module("...m", package="p.q")`, `importlib.import_module(".m", package=name)`, an f-string as the package, and `importlib.import_module(".m", package="")`
- **THEN** each is an unread site and none is an import

### Requirement: Relative import resolution
Relative imports SHALL resolve from the importing file's package, up to the root of the repository whatever project holds the file they reach, and one that climbs out of the repository SHALL be unresolved as not found.

#### Scenario: Relative imports climb packages
- **WHEN** `app/api/routes.py` has `from ..db import session` and `from . import deps`
- **THEN** they resolve to `app/db/session.py` or `app/db.py`, whichever exists, and to `app/api/deps.py`

#### Scenario: Relative imports may climb above the project
- **WHEN** `services/api/app/x.py` imports `util` from `..`, `common` from `...`, `tool` from `...shared`, `top` from `....` and `beyond` from `.....`, where `services/api` and `services/shared` each hold a `pyproject.toml`
- **THEN** they resolve to `services/api/util.py`, `services/common.py`, `services/shared/tool.py` and `top.py`, and the last one, which leaves the repository, is unresolved as not found

### Requirement: Standard library and dependencies
A standard library module SHALL resolve to a builtin, a module of a declared dependency to that package (by its normalised distribution name and the known import names of common distributions), and anything else to an unknown package or not found.

#### Scenario: Standard library, dependencies and unknown modules
- **WHEN** a project declares `PyYAML` and `fastapi` and a file imports `json`, `yaml`, `fastapi.routing`, `requests` and `app.missing`
- **THEN** `json` resolves to a builtin, `yaml` to the package `pyyaml`, `fastapi.routing` to the package `fastapi`, `requests` to an unknown package and `app.missing` to not found

### Requirement: HTTP routes
With the `fastapi` pack, `get`, `post`, `put`, `patch`, `delete`, `options`, `head`, `trace` and `api_route` decorators on a receiver made by `FastAPI()` or `APIRouter()` SHALL serve the path, and `include_router(router, prefix=…)` SHALL serve the included router's routes under the prefix, across files.

#### Scenario: A FastAPI router included with a prefix
- **WHEN** `app/api/bookings.py` has `router = APIRouter(prefix="/bookings")` and `@router.get("/{booking_id}")`, and `app/main.py` has `app.include_router(bookings.router, prefix="/api")`
- **THEN** `app/api/bookings.py` serves `http GET /api/bookings/{}` at the line of the decorator
- **AND** it serves no other path

#### Scenario: A FastAPI route with the TRACE method
- **WHEN** `app.py` has `router = APIRouter()`, `app = FastAPI()`, `@router.trace("/probe")` and `@app.trace("/echo")`
- **THEN** `app.py` serves `http TRACE /probe` at the line of the first decorator and `http TRACE /echo` at the line of the second

### Requirement: FastAPI router chains
A route on an `APIRouter` SHALL be served once for each chain of `include_router(router, prefix=…)` calls that reaches the router, at the path made by joining, from the outermost router inwards, the `prefix` of each call and then the `prefix` of the router it includes, and last the path of the route. A router that no call includes SHALL serve its own `prefix` joined with the path.

#### Scenario: A FastAPI router mounted through another router
- **WHEN** `app/api/bookings.py` has `router = APIRouter(prefix="/bookings")` and `@router.get("/{booking_id}")`, `app/api/v1.py` has `sub = APIRouter(prefix="/v1")` and `sub.include_router(bookings.router, prefix="/b")`, and `app/main.py` has `app.include_router(v1.sub, prefix="/api")`
- **THEN** `app/api/bookings.py` serves `http GET /api/v1/b/bookings/{}` and no other path

### Requirement: Re-exported routers
A router that a mount names through a package that re-exports it, with an import in the package's `__init__.py` such as `from .bookings import router as bookings_router`, SHALL be the router of the module the import names, so that its routes are served behind the mount.

#### Scenario: A router mounted through a re-export
- **WHEN** `app/api/__init__.py` has `from .bookings import router as bookings_router`, `app/api/bookings.py` has `router = APIRouter(prefix="/bookings")` and `@router.get("/{booking_id}")`, and `app/main.py` has `from app.api import bookings_router` and `app.include_router(bookings_router, prefix="/api")`
- **THEN** `app/api/bookings.py` serves `http GET /api/bookings/{}` and no other path

### Requirement: Repeated and cyclic mounts
A call that includes a router without a `prefix` SHALL add no second copy of its routes. A chain of calls SHALL be cut where it comes back to a router it has passed, where it is deeper than 32 routers and at a call whose `prefix` is not a literal. The routes SHALL be served by the chains that end at a router nobody includes, so a router every chain of which is cut serves none.

#### Scenario: A router is served once for each way it is mounted
- **WHEN** `app/main.py` has `app.include_router(health.router)`, `app.include_router(items.router)` and `app.include_router(items.router, prefix="/v2")`, and `@app.get("/ping")`, where `health.router` is `APIRouter()` with `@router.get("/health")` and `items.router` is `APIRouter(prefix="/items")` with `@router.get("/{item_id}")`
- **THEN** `app/health.py` serves `http GET /health` once, `app/items.py` serves `http GET /items/{}` and `http GET /v2/items/{}`, and `app/main.py` serves `http GET /ping`

#### Scenario: Routers that mount each other still serve their routes
- **WHEN** `app/a.py` has `router = APIRouter(prefix="/x")`, `router.include_router(b.router, prefix="/b")` and `@router.get("/one")`, `app/b.py` has `router = APIRouter(prefix="/y")`, `router.include_router(a.router, prefix="/a")` and `@router.get("/two")`, and `app/main.py` has `app.include_router(a.router, prefix="/a")`
- **THEN** `app/a.py` serves `http GET /a/x/one` and `app/b.py` serves `http GET /a/x/b/y/two`

#### Scenario: A router whose every chain is cut serves nothing
- **WHEN** `app/a.py` has `router = APIRouter(prefix="/x")`, `router.include_router(b.router, prefix="/b")` and `@router.get("/one")`, `app/b.py` has `router = APIRouter(prefix="/y")`, `router.include_router(a.router, prefix="/a")` and `@router.get("/two")`, and no file includes either router in an application
- **THEN** neither `app/a.py` nor `app/b.py` serves a route

#### Scenario: A prefix that is not a literal cuts the chain
- **WHEN** `app/items.py` has `router = APIRouter(prefix="/items")` and `@router.get("/{item_id}")`, and `app/main.py` has `PREFIX = "/api"`, `app.include_router(items.router, prefix=PREFIX)` and `app.include_router(items.router, prefix="/v2")`
- **THEN** `app/items.py` serves `http GET /v2/items/{}` and no other path
- **AND** the call with `prefix=PREFIX` is an unread site of `app/main.py`

### Requirement: Ways kept for a router
A router that more than 64 chains of mounts reach SHALL be served by the 64 whose paths sort first, among the first 1024 chains found, and the order of the mounts in the files SHALL not change which ones.

#### Scenario: A router reached in more than 64 ways is served in the first 64 by path
- **WHEN** a project that declares `fastapi` has seven layers of two routers, each included in both routers of the layer above with `prefix="/<layer><from><to>"`, so that the lowest router is reached in 128 ways, and `@a0.get("/leaf")` on the lowest router
- **THEN** that router serves the 64 first of the 128 paths in sorted order, the first being `http GET /6aa/5aa/4aa/3aa/2aa/1aa/0aa/leaf`
- **AND** it serves the same 64 when the calls are written in the opposite order

### Requirement: Mounts that are not followed
The provides of a project SHALL name, as one blind spot of the routes it serves, the mounts of a router that it does not follow: by a file of another Python project, in a cycle, deeper than 32 routers, in more than 64 ways, with a prefix held in a variable, through an import that cannot be resolved or through a name that a module other than a package's `__init__.py` re-exports. With the `fastapi` pack it SHALL name applications mounted under a path with `mount` as another.

#### Scenario: The mounts that are not followed are named
- **WHEN** the coverage of a project that declares `fastapi`, `flask` and `django` is read
- **THEN** its provides name each of those mounts in one blind spot of the routes it serves, and applications mounted under a path with `mount` in another

### Requirement: Flask routes
With the `flask` pack, `route` and the verb decorators on a receiver made by `Flask()` or `Blueprint()` SHALL serve the path, the method taken from the first entry of `methods`, `GET` when it is absent and any method when it is held in a name, a call or a `**` mapping, and `register_blueprint(bp, url_prefix=…)` SHALL serve the blueprint's routes under the prefix, across files.

#### Scenario: A Flask blueprint serves under its prefix
- **WHEN** a file has `bp = Blueprint("b", __name__, url_prefix="/bookings")` and `@bp.route("/<int:id>", methods=["DELETE"])`
- **THEN** it serves `http DELETE /bookings/{}`

#### Scenario: A Flask route whose methods are not text serves any method
- **WHEN** `app.py` has `METHODS = ["GET", "POST"]`, `@app.route("/held", methods=METHODS)`, `@app.route("/plain")` and `@app.route("/post", methods=["POST"])`
- **THEN** `app.py` serves `http * /held`, `http GET /plain` and `http POST /post`

#### Scenario: A Flask route whose options are unpacked serves any method
- **WHEN** `app.py` has `@app.route("/held", **OPTIONS)` on line 7 and `@app.route("/plain")` on line 12
- **THEN** `app.py` serves `http * /held` and `http GET /plain`

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
With the `requests` and `httpx` packs, the verb functions of `requests` and `httpx`, `requests.request(method, url)`, and the verbs on a receiver made by `requests.Session()`, `httpx.Client()` or `httpx.AsyncClient()`, held in a variable or called where made, SHALL call the URL with the verb. An f-string URL SHALL keep its literal parts with each computed part as a parameter; a URL with no literal path segment SHALL be an unread site, also on a receiver made with no base URL.

#### Scenario: Requests become calls
- **WHEN** a file has `requests.post(f"{BASE}/bookings/{id}/cancel")` and `httpx.get(url)`
- **THEN** it calls `http POST /bookings/{}/cancel` and has an unread site at the line of `httpx.get`

#### Scenario: A client with no base URL leaves a computed URL unread
- **WHEN** a file has `session = requests.Session()` and `session.get(f"{base}{path}")`, `httpx.Client().get(f"{url}")`, and `client = httpx.Client(base_url="https://api.example.com/v1")` with `client.get("/bookings")`
- **THEN** it has an unread site at the line of each of the first two calls
- **AND** it calls `http GET /v1/bookings`

#### Scenario: A URL whose path is only computed segments is a blind spot
- **WHEN** a file has `requests.get(f"{base}/{id}")` and `requests.get(f"{base}/users/{id}")`
- **THEN** the first is an unread site at its line
- **AND** the second calls `http GET /users/{}`

### Requirement: Tables and SQL
With the `sqlalchemy` pack, `__tablename__ = "<name>"` in the body of a class and `Table("<name>", …)` SHALL declare `table <name>`, and a string passed to `text()` SHALL be an embedded `sql` document. With the `django` pack, `db_table = "<name>"` in a class `Meta` nested in another class SHALL declare `table <name>`. The same names assigned anywhere else, as an attribute written with dots such as `self.__tablename__` included, SHALL declare nothing.

#### Scenario: Model tables are declared
- **WHEN** one class has `__tablename__ = "bookings"` and a Django model's `Meta` has `db_table = "booking_items"`
- **THEN** the files declare `table bookings` and `table booking_items`

#### Scenario: A table name outside a model declares nothing
- **WHEN** a file assigns `__tablename__` and `db_table` at module level and in a method, `db_table` in a class `Meta` that is in no class, and `db_table` in a class `Options` of a model
- **AND** it has a class with `__tablename__` in its body, a class with `__tablename__` inside another class, and models whose `Meta` has `db_table`, one nested in another class and one defined in a function
- **THEN** it declares only the tables of those class bodies and `Meta` classes

#### Scenario: An attribute assignment declares no table
- **WHEN** a file assigns `__tablename__` as an attribute of `self` in a method and of a class at module level, and `db_table` as an attribute of the `Meta` of a class at module level
- **AND** it has a class with `__tablename__` in its body
- **THEN** it declares only the table of that class body

### Requirement: Assignment targets
A pack's assignment rule SHALL match a name assigned in a class body as written and as qualified by the classes around it, as many as its target has segments before the last, so `*.Meta.db_table` is a `db_table` in a class `Meta` inside another class. A target of one segment SHALL match the name wherever it is assigned.

#### Scenario: A target of one segment matches wherever the name is assigned
- **WHEN** a pack's rule has the target `db_table` and a file assigns `db_table` at module level, in a method, in a class `Meta` that is in no class and in the `Meta` of a model
- **THEN** each of them declares a table

### Requirement: Tables that a class gets from elsewhere
With the `sqlalchemy` pack, a table name that a `declared_attr` function gives or that a model inherits from a mixin SHALL not be read, and the provides of the project SHALL name those table names as a blind spot of its tables.

#### Scenario: A table name given by a declared_attr is a blind spot
- **WHEN** a project with the `sqlalchemy` pack has a mixin whose `__tablename__` is a `declared_attr` function, and a model that derives from the mixin
- **THEN** the files declare no table
- **AND** the provides of the project name table names given by a `declared_attr` or inherited from a mixin as a blind spot

### Requirement: Alembic migrations
With the `alembic` pack, `op.create_table`, `op.drop_table`, `op.rename_table`, `op.add_column`, `op.drop_column`, `op.alter_column` and `op.create_index` SHALL declare `table <name>` for the table they name, whatever `op` is imported as.

#### Scenario: A migration declares its tables
- **WHEN** an Alembic revision has `op.create_table("bookings", sa.Column("id", sa.Integer))` and `op.add_column("booking_items", sa.Column("note", sa.Text))`
- **THEN** it declares `table bookings` and `table booking_items`

#### Scenario: A migration with an aliased op declares its tables
- **WHEN** an Alembic revision has `from alembic import op as o` and `from alembic.op import drop_table as drop`, and calls `o.create_table("bookings", …)`, `o.create_index("ix_items_note", "booking_items", ["note"])` and `drop("old_bookings")`
- **THEN** it declares `table bookings`, `table booking_items` and `table old_bookings`

### Requirement: DB-API SQL
With the `dbapi` pack, the first argument of `execute` and `executemany` on a receiver of up to three segments SHALL be an embedded `sql` document when it is a string literal. The document SHALL start on the line its first literal starts on and keep the lines of the source: each part of a literal joined by `+` or implicit concatenation stays on its own line, a line break in the source stays, and one an escape writes adds none; a `--` comment before it SHALL make the call an unread site.

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

#### Scenario: A line break in the source stays beside an escape
- **WHEN** a file has `cursor.execute(` on line 2 and a triple-quoted literal that opens on line 2, with `SELECT a -- note` on line 3, `FROM t` on line 4 and `WHERE b = 'x\ny'` on line 5, the `\n` written as an escape
- **THEN** it embeds an `sql` document at line 2
- **AND** counting its lines from line 2, `-- note` is on line 3, `FROM t` on line 4 and `WHERE b = 'x y'` on line 5

#### Scenario: A line continuation keeps the lines after it
- **WHEN** a file has `cursor.execute(` on line 2 and a triple-quoted literal that opens on line 2, with `SELECT a, \` ending line 3, `b` on line 4 and `FROM t` on line 5
- **THEN** it embeds an `sql` document at line 2
- **AND** counting its lines from line 2, `FROM t` is on line 5

#### Scenario: A comment before an escape-made line break is an unread site
- **WHEN** a file has `cursor.execute("SELECT a -- note\nFROM t")` and `cursor.execute("SELECT a\nFROM t -- note")`
- **THEN** the first call is an unread site and embeds no document
- **AND** the second call embeds an `sql` document with the text `SELECT a FROM t -- note`

### Requirement: DB-API arguments that are not literals
With the `dbapi` pack, an `execute` or `executemany` call whose first argument is a string built at run time, an f-string, `+` or `%` on a string, or `.format` on a string, SHALL be an unread site. A call whose first argument is a variable, an attribute, a subscript or a call SHALL record nothing, because a method with that name need not take SQL, and the provides of the project SHALL name SQL built by a function call as a blind spot of its tables.

#### Scenario: A SQL string built at run time is an unread site
- **WHEN** a file has `cursor.execute(f"SELECT id FROM {table}")`, `cursor.execute("SELECT id FROM " + table)`, `cursor.execute("SELECT id FROM bookings WHERE id = %s" % task)` and `.format` on a text joined with `+`, and one call with the plain literal `"SELECT id FROM bookings"`
- **THEN** each call built at run time is an unread site and embeds no document
- **AND** the call with the plain literal embeds an `sql` document

#### Scenario: A format template with a replacement field is an unread site
- **WHEN** a file has `cursor.execute("SELECT id FROM {}".format(table))`, `cursor.execute("SELECT id FROM {name} WHERE id = {0}".format(task, name=table))`, `cursor.execute("SELECT id FROM {:{width}}".format(table, width=3))` and `cursor.execute("SELECT '{{}}'".format())`
- **THEN** the first three calls are unread sites and embed no document
- **AND** the last call embeds an `sql` document with the text `SELECT '{}'`

#### Scenario: An argument that is not a string records nothing
- **WHEN** a file has `pool.execute(task)`, `cursor.execute(queries[0])`, `cursor.executemany(queries.insert, rows)`, `cursor.execute(compose(task))` and `cursor.execute()`
- **THEN** none of them is an unread site and none embeds a document
- **AND** the provides of the project name SQL built by a function call as a blind spot

### Requirement: DB-API receivers
With the `dbapi` pack, `execute` and `executemany` called on a receiver of more than three segments SHALL not be read, and the provides of every project SHALL name those calls and SQL held in a variable as blind spots of its tables.

#### Scenario: A receiver of more than three segments is not read
- **WHEN** a file calls `execute("SELECT id FROM bookings")` on `cursor`, `self.cursor`, `self.db.cursor` and `self.app.db.cursor`
- **THEN** the first three calls embed an `sql` document and the fourth does not
- **AND** the provides of the project name `execute` and `executemany` calls on a receiver of more than three segments as a blind spot

### Requirement: Environment variables
With the `env` pack, `os.environ["<NAME>"]`, `os.environ.get("<NAME>")`, `os.environ.setdefault("<NAME>", …)`, `os.getenv("<NAME>")` and each field of a class deriving from `BaseSettings` SHALL call `env <NAME>`, a field's name prefixed with the literal `env_prefix` of the class's `model_config` or inner `Config` when it has one. A field whose name would be prefixed with an `env_prefix` that is not a literal SHALL leave an unread site and no `env` call.

#### Scenario: Every way of reading the environment is seen
- **WHEN** a file reads `os.environ["A"]`, `os.environ.get("B")`, `os.getenv("C")` and declares `class Settings(BaseSettings): database_url: str`
- **THEN** it calls `env A`, `env B`, `env C` and `env DATABASE_URL`

#### Scenario: A settings prefix is applied
- **WHEN** a class deriving from `BaseSettings` has `model_config = SettingsConfigDict(env_prefix="APP_")` and a field `port: int`
- **THEN** the file calls `env APP_PORT`

#### Scenario: A settings prefix that is not a literal leaves its fields unread
- **WHEN** classes deriving from `BaseSettings` have an `env_prefix` held in a variable, in a `SettingsConfigDict` call, in a `model_config` dict and in an inner `Config`, and one of them has the field `token: str = Field(alias="API_TOKEN")`
- **AND** another class has the literal `env_prefix="LIT_"` and a field `mode`
- **THEN** each field without an alias under a prefix held in a variable leaves an unread site and no `env` call, and the file calls `env API_TOKEN` and `env LIT_MODE`

#### Scenario: Setting a default for an environment variable reads it
- **WHEN** a file calls `os.environ.setdefault("DJANGO_SETTINGS_MODULE", "project.settings")`, `environ.setdefault("PORT", "8000")` after `from os import environ`, and `os.environ.setdefault(NAME, "x")`
- **THEN** it calls `env DJANGO_SETTINGS_MODULE` and `env PORT`, and the call whose name is a variable leaves an unread site

### Requirement: Settings aliases
A field of a class deriving from `BaseSettings` that is assigned a call with a literal `env=` (pydantic 1), `validation_alias=` or `alias=` SHALL call `env <NAME>` in place of the variable its name gives, in that order, with no `env_prefix` put in front. A keyword that is `None` SHALL count as not written. Any other keyword that is not a literal SHALL leave an unread site and no `env` call for the field.

#### Scenario: A settings field with an alias is read under the alias
- **WHEN** a class deriving from `BaseSettings` with `env_prefix="APP_"` has `database_url: str = Field(alias="DB_URL")`, `region: str = Field("eu", alias="OLD_REGION", validation_alias="REGION")`, `mode: str = Field(validation_alias=AliasChoices("MODE", "APP_MODE"))`, `secret: str = Field(alias=TOKEN_NAME)` and `timeout: int = 30`
- **THEN** it calls `env DB_URL`, `env REGION` and `env APP_TIMEOUT`, and `mode` and `secret` each leave an unread site

#### Scenario: A settings field with a pydantic 1 env name is read under it
- **WHEN** a class deriving from `BaseSettings` with `env_prefix="APP_"` has `database_url: str = Field(env="DB_URL")`, `region: str = Field("eu", env="REGION", alias="OLD_REGION")` and `timeout: int = 30`
- **THEN** it calls `env DB_URL`, `env REGION` and `env APP_TIMEOUT`

#### Scenario: A None alias is no alias
- **WHEN** a class deriving from `BaseSettings` has `port: int = Field(8000, alias=None)` and `mode: str = Field("dev", validation_alias=None, alias="MODE_NAME")`
- **THEN** it calls `env PORT` and `env MODE_NAME`, and leaves no unread site

### Requirement: Settings subclasses
Only a class that names `BaseSettings` as one of its bases SHALL have its fields read. The fields of a class deriving from another settings class, and an alias written inside a field's annotation, SHALL be among the blind spots of the project.

#### Scenario: The fields of a settings subclass are a blind spot
- **WHEN** `class Base(BaseSettings)` has the field `log_level` and `class Production(Base)` has the field `replicas`
- **THEN** the file calls `env LOG_LEVEL` and not `env REPLICAS`, and the provides of the project name the fields of a settings class that derives from another settings class as a blind spot

### Requirement: Requirements includes
A requirements file SHALL be read together with the files it includes with `-r` or `--requirement`, each relative to the file that names it, so the distributions an included file declares count for the project. Each file SHALL be read once however many files include it. An include that is not in the repository SHALL be skipped, and a constraints file (`-c`) SHALL NOT be followed. An included file that is not named like a requirements file is read but not claimed.

#### Scenario: Requirements files include each other
- **WHEN** `requirements.txt` includes `requirements/dev.txt`, which includes `base.txt` beside it, which declares `fastapi` and includes `../common/shared.txt`, which declares `sqlalchemy` and includes `base.txt` back, and `dev.txt` also names two includes that are not in the repository
- **THEN** the `fastapi` and `sqlalchemy` packs are in force, so `app.py` serves `http GET /ping` and `models.py` declares `table bookings`, and `common/shared.txt` is read and not claimed

#### Scenario: A requirements directory holds manifests
- **WHEN** a project has `requirements.txt`, `requirements/base.txt` and `requirements/dev.txt`
- **THEN** the project claims all three and tags each as a manifest

### Requirement: Continued lines of a requirements file
A line of a requirements file that ends with a backslash SHALL be read together with the line after it, as pip joins them, before a comment is cut off or an include is found: `-r \` followed by `base.txt` includes `base.txt`. A comment line SHALL NOT continue, whatever it ends with, so the line after it is read on its own.

#### Scenario: A requirement continues on the next line
- **WHEN** `requirements.txt` has `-r \` followed by an indented `base.txt` that declares `fastapi`, then a comment line ending with a backslash followed by `sqlalchemy`, then `flask \` followed by an indented `>=3.0`
- **THEN** the `fastapi`, `sqlalchemy` and `flask` packs are in force

### Requirement: Manifests that are not regular files
A manifest or an included requirements file that is not a regular file, such as a symbolic link, SHALL NOT be read, even when the file set lists it, so that a link cannot bring in a file from outside the repository. The project SHALL still open, without what the file would have said.

#### Scenario: A manifest that is a symbolic link is not read
- **WHEN** `requirements.txt` includes `dev.txt`, which is a symbolic link to a file outside the repository that declares `fastapi`, and the file set lists the link
- **THEN** the `fastapi` pack is not in force, so `app.py` records no route

### Requirement: Keywords of a setup.py
The `install_requires` list and the `package_dir` dict of a `setup.py` SHALL be read only when a bracket of the right kind follows the keyword's `=` directly. A keyword whose value is a name or a call, and a bracket that belongs to a later keyword such as `extras_require` or `entry_points`, SHALL give nothing. A backslash in a string SHALL make the character after it part of the string.

#### Scenario: A setup.py keyword is read only when a literal follows it
- **WHEN** a project has a `setup.py` with `install_requires=REQS` and `extras_require={"dev": ["fastapi"]}` after it
- **AND** another has `package_dir=PKG_DIRS` and `entry_points={"": "lib"}` after it, and a third has `install_requires=["left\"right", "fastapi"]`
- **THEN** the first has no `fastapi` pack in force, the second has no source root `lib`, so `from bookings import api` does not resolve to `lib/bookings/api.py`, and the third has the `fastapi` pack in force

### Requirement: Pack activation
A pack SHALL be active in a project when its manifests declare one of the pack's `detect` packages, compared by normalised name, in `[project]` dependencies, optional dependencies, dependency groups, Poetry dependencies, the `install_requires` of `setup.py` and `setup.cfg` or a requirements file, or always when its `detect` is empty. Each project SHALL report what it provides, with its blind spots in words.

#### Scenario: Packs follow the manifest
- **WHEN** a project's `pyproject.toml` declares `Flask>=3` and not `fastapi`
- **THEN** Flask routes in it produce sites and FastAPI decorators do not

#### Scenario: A setup.py with bracketed requirements is read
- **WHEN** a project has a `setup.py` with `package_dir={"": "lib"}` and `install_requires=["fastapi[standard]>=0.1",  # don't pin the extras` and `"sqlalchemy"]`, and `lib/bookings/api.py` has an `APIRouter()` with `@router.get("/ping")`
- **THEN** the `fastapi` pack is in force, so `lib/bookings/api.py` serves `http GET /ping`, and `from bookings import api` in another file of the project resolves to `lib/bookings/api.py`

#### Scenario: Blind spots are reported
- **WHEN** the coverage of a Python project is read
- **THEN** its provides name, among the blind spots, Flask routes with several methods beyond the first, Flask routes whose methods are held in a variable, prefixes held in variables, Django default table names, `execute` and `executemany` calls on a receiver of more than three segments, SQL held in a variable, the fields of a settings class that derives from another settings class, and an alias written inside a field's annotation

### Requirement: Prefixes that are not text
A route on a router, a blueprint or a class whose own prefix is not text, such as `APIRouter(prefix=settings.API_PREFIX)`, SHALL serve nothing and be an unread site of its file, unless the `url_prefix` of the Flask registration it is served through replaces that prefix. A way through such a router SHALL be cut, and the include below it SHALL be an unread site. A mount whose prefix is not text SHALL be one unread site, whatever else stops the way at it.

#### Scenario: A router whose prefix is not text serves none of its routes
- **WHEN** `app/users.py` has `router = APIRouter(prefix=settings.API_PREFIX)`, `router.include_router(child.router)` on line 7, `@router.get("/users")` on line 10 and `@router.post("/users")` on line 15, `app/dynamic.py` has `router = APIRouter(prefix=get_prefix())` and `@router.get("/dynamic")`, `app/child.py` has `router = APIRouter(prefix="/child")` and `@router.get("/leaf")`, `app/ok.py` has `router = APIRouter(prefix="/ok")` and `@router.get("/ping")`, and `app/main.py` includes the routers of `users`, `dynamic` and `ok`
- **THEN** `app/users.py` and `app/dynamic.py` serve no route, and each decorator is an unread site of its file
- **AND** `app/child.py` serves no route, because the only way to it goes through the router of `app/users.py`, and the include on line 7 of `app/users.py` is an unread site
- **AND** `app/ok.py` serves `http GET /ok/ping`

#### Scenario: A registration prefix stands in for a blueprint prefix that is not text
- **WHEN** `api.py` has `api = Blueprint("api", __name__, url_prefix=settings.PREFIX)` and `@api.get("/x")`, `loose.py` has such a blueprint `loose` and `@loose.get("/y")`, `parent.py` has such a blueprint `parent` that registers `child`, a blueprint of `url_prefix="/c"` with `@child.get("/z")`, and `__init__.py` has `app.register_blueprint(api, url_prefix="/api")`, `app.register_blueprint(loose)` and `app.register_blueprint(parent, url_prefix="/v1")`
- **THEN** `api.py` serves `http GET /api/x` and has no unread site
- **AND** `loose.py` serves no route and its decorator is an unread site
- **AND** `child.py` serves `http GET /v1/c/z` and has no unread site

#### Scenario: A blueprint registered with a prefix and without is served and unread
- **WHEN** `app/api.py` has `api = Blueprint("api", __name__, url_prefix=settings.PREFIX)` on line 5 and `@api.get("/x")` on line 8, and `app/__init__.py` has `app.register_blueprint(api, url_prefix="/v1")` on line 6 and `app.register_blueprint(api)` on line 7
- **THEN** `app/api.py` serves `http GET /v1/x`
- **AND** the decorator on line 8 is an unread site as well, because the registration on line 7 keeps the prefix of the blueprint, which is not text

#### Scenario: A mount whose prefix is not text is one unread site
- **WHEN** `app/users.py` has `router = APIRouter()` and `@router.get("/users")`, and `app/main.py` has `app.include_router(users.router, prefix=settings.PREFIX)`
- **THEN** `app/users.py` serves no route
- **AND** the include in `app/main.py` is one unread site, not two

#### Scenario: A mount whose prefix is not text is not also the cycle it closes
- **WHEN** `project/urls.py` has `re_path(PREFIX, include("app.urls"))` on line 7 and `path("health/", views.health)`, and `app/urls.py` has `path("items/", views.items)` and `path("back/", include("project.urls"))`
- **THEN** the `re_path` on line 7 of `project/urls.py` is one unread site, named by the call it is written as
- **AND** the link adds no second site for the cycle that the include closes

#### Scenario: A class decorator whose prefix is not text leaves its routes unread
- **WHEN** a pack reads the prefix of a route from the argument of the decorator `controller` on its class, and `views.py` has `@controller(BASE)` on a class with `@route("/a")`, `@controller("/fixed")` on a class with `@route("/b")` and a bare `@controller` on a class with `@route("/c")`
- **THEN** `views.py` serves `http GET /fixed/b` and `http GET /c`
- **AND** the route of the first class is an unread site

### Requirement: Ways the link stops
A way from a route up through the routers that include it that the link stops, because the includes close a cycle or the chain is deeper or wider than the bounds of the link, SHALL leave the include it stops at as an unread site of its file, whose reason names the bound it hit.

#### Scenario: Routers that include each other leave the include an unread site
- **WHEN** `app/a.py` has `router = APIRouter(prefix="/a")`, `router.include_router(b.router)` on line 6 and `@router.get("/one")`, `app/b.py` has `router = APIRouter(prefix="/b")` and `router.include_router(a.router)`, and `app/main.py` includes `a.router`
- **THEN** `app/a.py` serves `http GET /a/one`
- **AND** the include on line 6 of `app/a.py`, where the cycle closes, is an unread site and `app/b.py` has none

### Requirement: Unpackings
A call with `*iterable` among its arguments or `**mapping` among its keywords, and a dict literal with `**mapping` among its entries, SHALL hold what they may give: a keyword the call does not write, the argument at or after the `*iterable`, and a key the dict does not write. What a call holds and no connector can read as text SHALL leave the value unread, so a method it may give is any method and a key it may give is an unread site.

#### Scenario: An unpacking may carry a key or a keyword
- **WHEN** a pack calls `http` for `client.request` with the key `$arg1|$url` and the method `$arg2.method|$verb|GET`, and `calls.py` has `client.request("/plain")` on line 2, `client.request("/post", {"method": "POST"})` on line 3, `client.request("/dict", {**base})` on line 4, `client.request("/keyword", **options)` on line 5, `client.request("/positional", *rest)` on line 6 and `client.request(*parts)` on line 7
- **THEN** `calls.py` calls `http GET /plain`, `http POST /post`, `http * /dict`, `http * /keyword` and `http * /positional`
- **AND** the call on line 7 is an unread site

### Requirement: Environment writes
With the `env` pack, `os.environ["<NAME>"] = …`, an augmented assignment to it, `del os.environ["<NAME>"]`, `os.putenv`, `os.unsetenv`, `monkeypatch.setenv` and `monkeypatch.delenv` with a literal name SHALL serve `env <NAME>` and not call it. `os.environ.setdefault` and `os.environ.pop` SHALL call and serve it. A name that is not a literal SHALL leave one unread site per call.

#### Scenario: A write of an environment variable serves it
- **WHEN** a file has `os.environ["A"] = "x"`, `os.environ["B"] += ":y"`, `del os.environ["C"]`, `os.putenv("D", "z")`, `os.unsetenv("E")`, and a function that calls `monkeypatch.setenv("F", "1")` and `monkeypatch.delenv("G")`
- **THEN** it serves `env A` to `env G`, calls no `env` and leaves no unread site

#### Scenario: Setting a default or popping a variable calls and serves it
- **WHEN** a file has `os.environ.setdefault("PORT", "8000")` and `os.environ.pop("TOKEN", None)`
- **THEN** it calls and serves `env PORT` and `env TOKEN`

#### Scenario: A computed name in a write is one unread site
- **WHEN** a file has `os.environ[NAME] = "x"`, `os.environ.setdefault(NAME, "y")` and `os.environ.pop(NAME, None)`
- **THEN** it names no `env` and leaves one unread site for each of the three lines

### Requirement: A write below a variable
A site that assigns to a member of the value of an environment variable, as `os.environ["P"].foo = 1`, SHALL call `env P` and not serve it, because it sets no variable.

#### Scenario: A write to a member of a variable reads it
- **WHEN** a file has `os.environ["P"].foo = 1`
- **THEN** it calls `env P` and serves no `env`

### Requirement: Relative requests
With the `requests` and `httpx` packs, a request whose URL is relative and has no base URL SHALL be an unread site, since it fails at run time: `requests.get("users")`, `requests.Session().get("users/me")` and `httpx.Client().get("items")`. A client whose base URL is there and is not text SHALL call the path it names. An absolute URL SHALL be read as before.

#### Scenario: A relative request with no base URL is unread
- **WHEN** a file has `requests.get("users")` on line 8, `requests.Session().get("users/me")` on line 12 and `httpx.Client().get("items")` on line 16
- **THEN** each is an unread site at its line

#### Scenario: A client with an unread base URL calls the relative path
- **WHEN** a file has `BASE = settings.API` and `httpx.Client(base_url=BASE).get("items")`
- **THEN** it calls `http GET /items`

#### Scenario: An absolute request is unchanged by relative requests
- **WHEN** a file has `requests.get("https://a.test/x")`
- **THEN** it calls `http GET /x`

### Requirement: Packs that find SQL in the provides
The provides of a project SHALL name every pack whose `[[embeds]]` rules find SQL, as well as the packs whose other rules name a table, in `via` of the `table` calls.

#### Scenario: The dbapi pack is credited with the tables it finds
- **WHEN** a project with no manifest has a file that calls `cursor.execute("SELECT id FROM bookings")`
- **THEN** the provides name `dbapi` in `via` of the `table` calls

#### Scenario: The sqlalchemy pack is credited with the tables it finds
- **WHEN** a project that depends on `sqlalchemy` has a file that calls `connection.execute(text("SELECT id FROM bookings"))`
- **THEN** the provides name `dbapi` and `sqlalchemy` in `via` of the `table` calls

### Requirement: A call rule that lists receiver types reads nothing
A call rule with `receiver_types` SHALL read nothing in Python, because the connector knows no declared type and reading every receiver would make false edges.

#### Scenario: A call rule that lists receiver types reads nothing
- **WHEN** a pack has a call rule on `client.send` with `receiver_types` and a file calls `client.send("topic")`
- **THEN** the file calls no contract and has no unread site

### Requirement: Environment writes through a mapping
A call that writes keys into a mapping the env pack roots SHALL serve each literal key it sets: `patch.dict`, as a call, in a `with` or as a decorator, `monkeypatch.setitem`, `monkeypatch.delitem`, `.update` and `|=`; the mapping may be named by a string. A computed key, a `**` item and values that are no dict literal SHALL leave one unread site per call. A write into another mapping, and one with no keys, SHALL name nothing.

#### Scenario: patch.dict serves the keys and keywords it sets
- **WHEN** a file has `patch.dict` of `os.environ` with the dict `{"A": "1", "B": "2"}` in a `with`, with `values={"C": "3"}` and `D="4"`, with `in_dict=os.environ` and `E="5"`, and with `{"F": "6"}` and `clear=True`, and a call `patch.dict(os.environ, clear=True)`
- **THEN** it serves `env A` to `env F`, calls no `env` and leaves no unread site

#### Scenario: patch.dict as a decorator serves its keys
- **WHEN** a file has `@patch.dict(os.environ, {"A": "1"})` on a function and `@patch.dict(os.environ, {"B": "2"}, C="3")` on a class
- **THEN** it serves `env A`, `env B` and `env C` on the lines of the decorators and leaves no unread site

#### Scenario: A mapping named by a string is written
- **WHEN** a file has `patch.dict("os.environ", {"A": "1"})` and `patch.dict("settings", {"B": "2"})`
- **THEN** it serves `env A` only and leaves no unread site

#### Scenario: A mapping write without values names nothing
- **WHEN** a file has `patch.dict(os.environ)`, `patch.dict(os.environ, clear=True)`, `os.environ.update()` and `print(os.environ)`
- **THEN** it names no `env` and leaves no unread site

#### Scenario: A computed key in a mapping write is one unread site per call
- **WHEN** a file has `patch.dict(os.environ, {"A": "1", NAME: "2", **BASE, OTHER: "3"})`, `patch.dict(os.environ, {"B": "1"}, **extra)`, `patch.dict(os.environ, values)` and `patch.dict(os.environ, dict(C="1"))`
- **THEN** it serves `env A` and `env B`, and each of the four calls is one unread site

#### Scenario: monkeypatch.setitem and delitem write one key
- **WHEN** a file has `monkeypatch.setitem(os.environ, "A", "1")` and `monkeypatch.delitem(os.environ, "B")`, and the same calls with the key `NAME`
- **THEN** it serves `env A` and `env B`, and each call with `NAME` is one unread site

#### Scenario: update and |= serve the keys they set
- **WHEN** a file has `os.environ.update({"A": "1"}, B="2")`, `environ |= {"C": "3"}` after `from os import environ`, `os.environ |= {"D": "4", NAME: "5"}`, `os.environ.update(extra)` and `os.environ.update({"E": "6"}, **extra)`
- **THEN** it serves `env A` to `env E`, and the last three calls are one unread site each

#### Scenario: A write into another mapping names nothing
- **WHEN** a file has `patch.dict(settings, {"A": "1", NAME: "2"}, B="3")`, `d.update({"C": 1}, **extra)`, `d |= {"D": 1}`, `monkeypatch.setitem(config, "E", 1)` and `monkeypatch.setitem(config, NAME, 1)`
- **THEN** it names no `env` and leaves no unread site

#### Scenario: A read inside the values of a mapping write is still a read
- **WHEN** a file has `patch.dict(os.environ, {"A": os.environ["B"]})`
- **THEN** it serves `env A` and calls `env B`
