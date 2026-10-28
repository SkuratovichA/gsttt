from app import bookings
from app.v1 import sub

sub.include_router(bookings.router, prefix="/b")
