from flask import Blueprint

from . import settings

api = Blueprint("api", __name__, url_prefix=settings.PREFIX)


@api.get("/x")
def x():
    return {}
