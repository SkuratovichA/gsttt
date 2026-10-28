import httpx
import requests

BASE = "https://api.example.com"


def cancel(id):
    requests.post(f"{BASE}/bookings/{id}/cancel")


def load(url):
    return httpx.get(url)
