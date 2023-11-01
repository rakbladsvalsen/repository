from __future__ import annotations
import logging
from typing import Optional
from httpx import Response, HTTPStatusError

from pydantic import BaseModel, Field

logger = logging.getLogger(__name__)


class RepositoryError(BaseModel):
    status_code: int = Field(None, alias="statusCode")
    kind: str
    detail: str

    @staticmethod
    def _try_extract_request_id(response: Response) -> Optional[str]:
        try:
            return response.headers.get("request-id", None)
        except Exception as e:
            logger.error("Couldn't extract request ID: %s", e, exc_info=e)
        return None

    @staticmethod
    def verify_raise_conditionally(response: Response):
        assert isinstance(response, Response), "not a `Response` object"
        try:
            logger.debug("checking response: [%s]: %s", response.url, response.headers)
            response.raise_for_status()
        except HTTPStatusError as err:
            try:
                # Try to deserialize error.
                error: RepositoryError = RepositoryError.model_validate(response.json())
            except Exception as nested:
                logger.error(
                    "Server response was: status: %s, response: %s",
                    response.status_code,
                    response.text,
                )
                raise RuntimeError(
                    f"Couldn't parse JSON error response: '{response.text}'"
                ) from nested

            request_id = RepositoryError._try_extract_request_id(response)

            logger.error(
                "Something went sideways. request id: %s, code: %s, text: %s",
                request_id,
                response.status_code,
                response.text,
            )
            raise RepositoryException(error, request_id) from err

        except RepositoryException as err:
            raise err from err


class BaseRepositoryException(Exception):
    def __init__(self, error: RepositoryError, request_id: str):
        message = f"[{error.status_code}] [{request_id}] {error.kind}: {error.detail}"
        logger.error("error: %s", message)
        super().__init__(message)
        self.error = error
        self.request_id = request_id


class RepositoryException(BaseRepositoryException):
    pass
