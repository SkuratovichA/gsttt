from flask import Flask

from .bookings import bp

app = Flask(__name__)
app.register_blueprint(bp, url_prefix="/api")
