import requests


def only_computed(base, id):
    return requests.get(f"{base}/{id}")


def with_literal(base, id):
    return requests.get(f"{base}/users/{id}")
