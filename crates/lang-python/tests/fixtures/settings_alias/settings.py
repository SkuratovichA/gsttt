from pydantic import AliasChoices, Field
from pydantic_settings import BaseSettings, SettingsConfigDict

TOKEN_NAME = "API_TOKEN"


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_prefix="APP_")

    database_url: str = Field(alias="DB_URL")
    api_token: str = Field(validation_alias="SERVICE_TOKEN")
    region: str = Field("eu", alias="OLD_REGION", validation_alias="REGION")
    mode: str = Field(validation_alias=AliasChoices("MODE", "APP_MODE"))
    secret: str = Field(alias=TOKEN_NAME)
    debug: bool = Field(False, description="Verbose logs")
    timeout: int = 30
