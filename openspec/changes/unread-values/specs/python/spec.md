# Spec Delta

## ADDED Requirements

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

## MODIFIED Requirements

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
