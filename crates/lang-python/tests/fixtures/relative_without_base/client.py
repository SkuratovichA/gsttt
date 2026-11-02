import httpx
import requests

BASE = settings.API


def module_level():
    return requests.get("users")


def session():
    return requests.Session().get("users/me")


def client():
    return httpx.Client().get("items")


def unread_base():
    return httpx.Client(base_url=BASE).get("items")


def absolute():
    return requests.get("https://a.test/x")
