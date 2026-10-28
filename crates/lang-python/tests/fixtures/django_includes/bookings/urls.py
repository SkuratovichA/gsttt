from django.urls import path

from . import views

urlpatterns = [
    path("bookings/<int:pk>/", views.detail),
]
