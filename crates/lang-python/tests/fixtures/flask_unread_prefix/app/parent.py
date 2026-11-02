from flask import Blueprint

from . import settings
from .child import child

parent = Blueprint("parent", __name__, url_prefix=settings.PREFIX)
parent.register_blueprint(child)
