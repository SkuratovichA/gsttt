from fastapi import APIRouter

from app.api import bookings

sub = APIRouter(prefix="/v1")
sub.include_router(bookings.router, prefix="/b")
