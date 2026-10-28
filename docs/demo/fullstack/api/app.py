from fastapi import FastAPI

from api.users import router

app = FastAPI()
app.include_router(router)
