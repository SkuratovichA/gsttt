from pydantic_settings import BaseSettings


class Base(BaseSettings):
    log_level: str = "info"


class Production(Base):
    replicas: int = 3
