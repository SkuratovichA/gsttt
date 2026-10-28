from flask import Blueprint

bp = Blueprint("bookings", __name__)


@bp.get("/bookings/<int:id>")
def show(id):
    return {"id": id}
