from flask import Blueprint

child = Blueprint("child", __name__, url_prefix="/c")


@child.get("/z")
def z():
    return {}
