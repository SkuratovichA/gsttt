from django.urls import path as p

from . import views

urlpatterns = [
    p("bookings/<int:pk>/", views.detail),
]
