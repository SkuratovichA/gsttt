from fastapi import FastAPI

from app import a

app = FastAPI()
app.include_router(a.router, prefix="/a")
