from flask import Flask

from .bookings import bp
from .reports import reports

app = Flask(__name__)
app.register_blueprint(bp, url_prefix="/api")
app.register_blueprint(reports)
