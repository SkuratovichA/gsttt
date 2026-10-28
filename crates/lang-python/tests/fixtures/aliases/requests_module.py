import requests as r

BASE = "https://api.example.com"


def cancel(id):
    r.post(f"{BASE}/bookings/{id}/cancel")
