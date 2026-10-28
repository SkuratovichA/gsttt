from django.urls import include as inc
from django.urls import path as p

urlpatterns = [
    p("api/", inc("bookings.urls")),
]
