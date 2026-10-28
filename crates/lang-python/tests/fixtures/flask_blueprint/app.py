from flask import Blueprint

bp = Blueprint("b", __name__, url_prefix="/bookings")


@bp.route("/<int:id>", methods=["DELETE"])
def cancel(id):
    return "", 204
