from pydantic import PrivateAttr
from repoclient.models.base_model import ClientBaseModel
from httpx import Response
from typing import Optional
import logging

from repoclient.exception import (
    RepositoryError,
    RepositoryKindError,
    RepositoryException,
)

logger = logging.getLogger("repoclient")


class RequestModel(ClientBaseModel):
    _error: RepositoryError = PrivateAttr(None)
    _request_id: Optional[str] = PrivateAttr(None)

    @staticmethod
    def _try_extract_request_id(response: Response):
        try:
            return response.headers.get("request-id", None)
        except Exception as e:
            logger.error("Couldn't extract request ID: %s", e, exc_info=e)
        return None

    def handle_exception(self, response: Response):
        try:
            self._error = RepositoryError.parse_obj(response.json())
        except Exception as err:
            logger.error(
                "Server response was: status: %s, response: %s",
                response.status_code,
                response.text,
            )
            logger.error(
                "Couldn't deserialize JSON error response: %s", response, exc_info=err
            )
            raise RuntimeError("Couldn't parse JSON error response")
        self._request_id = self._try_extract_request_id(response)
        logger.error("something went sideways. request ID was %s", self._request_id)
        self._raise_err()

    def _raise_err(self):
        if not isinstance(self._error.kind, RepositoryKindError):
            logger.error("Unhandled upstream error type: %s", self._error.kind)
        raise RepositoryException(self._error, self._request_id)
