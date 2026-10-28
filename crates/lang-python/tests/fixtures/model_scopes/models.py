from django.db import models
from sqlalchemy.orm import DeclarativeBase

__tablename__ = "module_level"
db_table = "module_variable"


class Base(DeclarativeBase):
    pass


class Booking(Base):
    __tablename__ = "bookings"

    def rename(self):
        __tablename__ = "in_a_method"
        db_table = "in_a_method_too"


class Namespace:
    class Inner(Base):
        __tablename__ = "inner"

    class Item(models.Model):
        class Meta:
            db_table = "deep"


class BookingItem(models.Model):
    class Meta:
        db_table = "booking_items"


class Meta:
    db_table = "meta_alone"


class Audit(models.Model):
    class Options:
        db_table = "options_not_meta"

    class Meta(Base):
        db_table = "audit_log"


def declare():
    class Local(models.Model):
        class Meta:
            db_table = "in_a_function"

    return Local
