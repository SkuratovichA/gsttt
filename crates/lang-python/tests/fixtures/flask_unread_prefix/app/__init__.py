from flask import Flask

from .api import api
from .loose import loose
from .parent import parent

app = Flask(__name__)
app.register_blueprint(api, url_prefix="/api")
app.register_blueprint(loose)
app.register_blueprint(parent, url_prefix="/v1")
