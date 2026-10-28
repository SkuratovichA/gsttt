from flask import Blueprint

from .reports import reports

admin = Blueprint("admin", __name__, url_prefix="/admin")
admin.register_blueprint(reports)
