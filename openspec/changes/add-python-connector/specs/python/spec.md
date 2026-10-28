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
- **WHEN** a project has `pyproject.toml` and `src/booking/api.py`, and `tests/test_api.py` imports `booking.api`
- **THEN** the import resolves to `src/booking/api.py`

### Requirement: Imports
The connector SHALL read `import a.b`, `import a.b as c`, `from a.b import c`, relative imports with any number of dots, and `importlib.import_module` and `__import__` with a literal argument, each with its line and names. Imports under `if TYPE_CHECKING:` SHALL be flagged type-checking and imports inside a `try` whose handler catches `ImportError` or `ModuleNotFoundError` SHALL be flagged optional. `importlib.import_module` with a computed argument SHALL be an unread site.

#### Scenario: Every import form is read
- **WHEN** a file holds one import of each form listed above
- **THEN** its facts hold each import with its kind, line and names, matching the reviewed snapshot

#### Scenario: Guarded imports are flagged
- **WHEN** a file imports `a` under `if TYPE_CHECKING:` and `b` in `try: … except ImportError:`
- **THEN** the import of `a` is flagged type-checking and the import of `b` optional

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
A module path SHALL resolve to the deepest file it names under a source root: `a/b.py`, else `a/b/__init__.py`, else `a/b.pyi`, with namespace packages allowed. `from a import b` SHALL resolve to the module `a.b` when it exists and to `a` otherwise.

#### Scenario: Submodules and names are told apart
- **WHEN** `app/x.py` has `from app import models` and `from app.models import Booking`, and `app/models.py` exists
- **THEN** both imports resolve to `app/models.py`
- **AND** `from app import settings`, where `app/settings.py` does not exist but `app/__init__.py` does, resolves to `app/__init__.py`

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
With the `fastapi` pack, `get`, `post`, `put`, `patch`, `delete`, `options`, `head` and `api_route` decorators on a receiver made by `FastAPI()` or `APIRouter()` SHALL serve the receiver's `prefix` joined with the path, and `include_router(router, prefix=…)` SHALL serve the included router's routes under the prefix, across files.

#### Scenario: A FastAPI router included with a prefix
- **WHEN** `app/api/bookings.py` has `router = APIRouter(prefix="/bookings")` and `@router.get("/{booking_id}")`, and `app/main.py` has `app.include_router(bookings.router, prefix="/api")`
- **THEN** `app/api/bookings.py` serves `http GET /api/bookings/{}` at the line of the decorator

### Requirement: Flask routes
With the `flask` pack, `route` and the verb decorators on a receiver made by `Flask()` or `Blueprint()` SHALL serve the `url_prefix` joined with the path, the method taken from the first entry of `methods` and `GET` when it is absent, and `register_blueprint(bp, url_prefix=…)` SHALL add its prefix across files.

#### Scenario: A Flask blueprint serves under its prefix
- **WHEN** a file has `bp = Blueprint("b", __name__, url_prefix="/bookings")` and `@bp.route("/<int:id>", methods=["DELETE"])`
- **THEN** it serves `http DELETE /bookings/{}`

### Requirement: Django routes
With the `django` pack, `path()` entries in `urlpatterns` SHALL serve their routes, with `include("<module>")` adding the including route as a prefix across files.

#### Scenario: Django includes compose
- **WHEN** `project/urls.py` has `path("api/", include("bookings.urls"))` and `bookings/urls.py` has `path("bookings/<int:pk>/", views.detail)`
- **THEN** `bookings/urls.py` serves `http * /api/bookings/{}` at the line of its `path` entry

### Requirement: HTTP requests
With the `requests` and `httpx` packs, `requests.get` and the other verbs, `requests.request(method, url)`, the same verbs on `httpx` and on receivers made by `httpx.Client()` or `httpx.AsyncClient()` SHALL call the URL with the verb. An f-string URL SHALL keep its literal parts with each computed part as a parameter, and a URL with no literal path SHALL be an unread site.

#### Scenario: Requests become calls
- **WHEN** a file has `requests.post(f"{BASE}/bookings/{id}/cancel")` and `httpx.get(url)`
- **THEN** it calls `http POST /bookings/{}/cancel` and has an unread site at the line of `httpx.get`

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
With the `dbapi` pack, the first argument of `execute` and `executemany` on any receiver SHALL be an embedded `sql` document when it is a string literal.

#### Scenario: Raw SQL is embedded
- **WHEN** a file has `cursor.execute("SELECT id FROM bookings WHERE id = %s", (id,))` on line 7
- **THEN** it embeds an `sql` document at line 7 with that text

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
- **THEN** its provides name, among the blind spots, Flask routes with several methods beyond the first, prefixes held in variables and Django default table names
