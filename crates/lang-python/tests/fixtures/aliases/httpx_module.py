import httpx as hx

BASE = "https://api.example.com"


def load(id):
    return hx.get(f"{BASE}/bookings/{id}")


def with_base_url():
    client = hx.Client(base_url="https://api.example.com/v1")
    return client.get("/bookings")
