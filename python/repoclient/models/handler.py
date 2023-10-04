from pydantic import PrivateAttr
from repoclient.models.base_model import ClientBaseModel
from typing import Optional
import logging

from repoclient.exception import (
    RepositoryError,
)

logger = logging.getLogger("repoclient")


class RequestModel(ClientBaseModel):
    _error: RepositoryError = PrivateAttr(None)
    _request_id: Optional[str] = PrivateAttr(None)
