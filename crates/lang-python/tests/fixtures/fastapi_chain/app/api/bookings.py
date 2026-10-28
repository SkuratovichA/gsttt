from fastapi import APIRouter

router = APIRouter(prefix="/bookings")


@router.get("/{booking_id}")
def get_booking(booking_id: int):
    return {"id": booking_id}
