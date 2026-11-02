# Spec Delta

## ADDED Requirements

### Requirement: Prefixes that are not text
A route on a router, a blueprint or a class whose own prefix is not text, such as `APIRouter(prefix=settings.API_PREFIX)` or `APIRouter(prefix=get_prefix())`, SHALL be an unread site of its file and serve nothing, since the path it is served at is not known. A chain of mounts SHALL be cut at such a router, so a router included into it is served by no chain that passes it.

#### Scenario: A router whose prefix is not text serves none of its routes
- **WHEN** `app/users.py` has `router = APIRouter(prefix=settings.API_PREFIX)`, `router.include_router(child.router)`, `@router.get("/users")` on line 10 and `@router.post("/users")` on line 15, `app/dynamic.py` has `router = APIRouter(prefix=get_prefix())` and `@router.get("/dynamic")`, `app/child.py` has `router = APIRouter(prefix="/child")` and `@router.get("/leaf")`, `app/ok.py` has `router = APIRouter(prefix="/ok")` and `@router.get("/ping")`, and `app/main.py` includes the routers of `users`, `dynamic` and `ok`
- **THEN** `app/users.py` and `app/dynamic.py` serve no route, and each decorator is an unread site of its file
- **AND** `app/child.py` serves no route, because the only way to it goes through the router of `app/users.py`
- **AND** `app/ok.py` serves `http GET /ok/ping`

#### Scenario: A class decorator whose prefix is not text leaves its routes unread
- **WHEN** a pack reads the prefix of a route from the argument of the decorator `controller` on its class, and `views.py` has `@controller(BASE)` on a class with `@route("/a")`, `@controller("/fixed")` on a class with `@route("/b")` and a bare `@controller` on a class with `@route("/c")`
- **THEN** `views.py` serves `http GET /fixed/b` and `http GET /c`
- **AND** the route of the first class is an unread site

## MODIFIED Requirements

### Requirement: Flask routes
With the `flask` pack, `route` and the verb decorators on a receiver made by `Flask()` or `Blueprint()` SHALL serve the path, the method taken from the first entry of `methods`, `GET` when it is absent and any method when it is held in a name or a call, and `register_blueprint(bp, url_prefix=…)` SHALL serve the blueprint's routes under the prefix, across files.

#### Scenario: A Flask blueprint serves under its prefix
- **WHEN** a file has `bp = Blueprint("b", __name__, url_prefix="/bookings")` and `@bp.route("/<int:id>", methods=["DELETE"])`
- **THEN** it serves `http DELETE /bookings/{}`

#### Scenario: A Flask route whose methods are not text serves any method
- **WHEN** `app.py` has `METHODS = ["GET", "POST"]`, `@app.route("/held", methods=METHODS)`, `@app.route("/plain")` and `@app.route("/post", methods=["POST"])`
- **THEN** `app.py` serves `http * /held`, `http GET /plain` and `http POST /post`
