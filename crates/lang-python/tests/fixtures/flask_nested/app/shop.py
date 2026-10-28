from flask import Blueprint

from .items import items

shop = Blueprint("shop", __name__, url_prefix="/shop")
shop.register_blueprint(items, url_prefix="/goods")
