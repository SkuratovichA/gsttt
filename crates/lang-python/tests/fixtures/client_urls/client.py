import httpx
import requests


def with_session(base, path):
    session = requests.Session()
    return session.get(f"{base}{path}")


def with_client(url):
    return httpx.Client().get(f"{url}")


def with_base_url():
    client = httpx.Client(base_url="https://api.example.com/v1")
    return client.get("/bookings")


def with_path():
    session = requests.Session()
    return session.get("/bookings/1")
