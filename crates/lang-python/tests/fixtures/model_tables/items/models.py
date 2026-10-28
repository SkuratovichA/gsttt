from django.db import models


class BookingItem(models.Model):
    note = models.TextField()

    class Meta:
        db_table = "booking_items"
