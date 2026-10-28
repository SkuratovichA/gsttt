from sqlalchemy.orm import DeclarativeBase


class Booking(DeclarativeBase):
    __tablename__ = "bookings"
