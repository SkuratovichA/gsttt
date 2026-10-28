from sqlalchemy.orm import DeclarativeBase


class Base(DeclarativeBase):
    pass


class Booking(Base):
    __tablename__ = "bookings"
