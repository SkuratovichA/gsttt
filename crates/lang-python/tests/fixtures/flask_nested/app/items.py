from flask import Blueprint

items = Blueprint("items", __name__, url_prefix="/items")


@items.get("/<int:id>")
def show(id):
    return {"id": id}
