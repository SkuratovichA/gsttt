from flask import Blueprint

from . import settings

loose = Blueprint("loose", __name__, url_prefix=settings.PREFIX)


@loose.get("/y")
def y():
    return {}
