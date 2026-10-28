from fastapi import FastAPI

from app import v1, wiring

app = FastAPI()
app.include_router(v1.sub, prefix="/api")
