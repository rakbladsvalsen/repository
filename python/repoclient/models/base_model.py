from pydantic import ConfigDict, BaseModel


class ClientBaseModel(BaseModel):
    # TODO[pydantic]: The following keys were removed: `json_loads`.
    # Check https://docs.pydantic.dev/dev-v2/migration/#changes-to-config for more information.
    model_config = ConfigDict(populate_by_name=True)
