from fastapi import FastAPI

from app.api import v1

app = FastAPI()
app.include_router(v1.sub, prefix="/api")
