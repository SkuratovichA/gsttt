from flask import Blueprint

bp = Blueprint("bookings", __name__, url_prefix="/bookings")


@bp.get("/<int:id>")
def show(id):
    return {"id": id}
