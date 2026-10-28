import fastapi as fa
from fastapi import APIRouter as R

app = fa.FastAPI()
router = R(prefix="/bookings")


@app.get("/ping")
def ping():
    return "pong"


@router.get("/{booking_id}")
def get_booking(booking_id: int):
    return {"id": booking_id}
