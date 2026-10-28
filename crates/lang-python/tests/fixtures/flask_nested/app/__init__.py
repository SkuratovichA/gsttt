from flask import Flask

from .admin import admin
from .shop import shop

app = Flask(__name__)
app.register_blueprint(admin)
app.register_blueprint(shop, url_prefix="/store")
