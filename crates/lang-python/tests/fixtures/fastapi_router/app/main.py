from fastapi import FastAPI

from app.api import bookings

app = FastAPI()
app.include_router(bookings.router, prefix="/api")
