from fastapi import APIRouter
from flask import Flask

app = Flask(__name__)
router = APIRouter(prefix="/x")


@app.route("/flask")
def flask_view():
    return "ok"


@router.get("/fast")
def fast_view():
    return {}
