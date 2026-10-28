import os

from pydantic_settings import BaseSettings

A = os.environ["A"]
B = os.environ.get("B")
C = os.getenv("C")


class Settings(BaseSettings):
    database_url: str
