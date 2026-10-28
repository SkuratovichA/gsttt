from fastapi import FastAPI

from app import health, items

app = FastAPI()
app.include_router(health.router)
app.include_router(items.router)
app.include_router(items.router, prefix="/v2")


@app.get("/ping")
def ping():
    return "pong"
