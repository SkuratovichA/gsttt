from requests import get
from requests import post as send

BASE = "https://api.example.com"


def load(id):
    return get(f"{BASE}/bookings/{id}")


def cancel(id):
    return send(f"{BASE}/bookings/{id}/cancel")
