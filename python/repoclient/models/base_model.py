import orjson.orjson
from pydantic import BaseModel


class ClientBaseModel(BaseModel):
    class Config:
        allow_population_by_field_name = True
        json_loads = orjson.orjson.loads
