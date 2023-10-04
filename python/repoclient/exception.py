from __future__ import annotations
import logging
from enum import Enum
from httpx import Response, HTTPStatusError

from pydantic import BaseModel, Field

logger = logging.getLogger(__name__)


class RepositoryKindError(str, Enum):
    DUPLICATE_ERROR = "DuplicateError"
    BAD_REQUEST = "DuplicateError"
    VALIDATION_FAILURE = "ValidationFailure"
    SERVER_ERROR = "ServerError"
    NOT_FOUND = "NotFound"
    INACTIVE_USER = "InactiveUser"
    INVALID_CREDENTIALS = "InvalidCredentials"
    INVALID_TOKEN = "InvalidToken"
    MISSING_AUTH_HEADER = "MissingAuthHeader"
    ADMIN_ONLY = "AdminOnlyResource"
    INSUFFICIENT_PERM = "InsufficientPermissions"
    INVALID_OPERATION = "InvalidOperation"
    CONFLICTING_OPERATION = "ConflictingOperation"
    CAST_ERROR = "CastError"
    INVALID_QUERY = "InvalidQuery"
    INVALID_PAGE_SIZE = "InvalidPageSize"
    UNHANDLED_FATAL_ERROR = "<FATAL UNHANDLED ERROR>"


class RepositoryError(BaseModel):
    status_code: int = Field(None, alias="statusCode")
    kind: RepositoryKindError | str
    detail: str

    @staticmethod
    def _try_extract_request_id(response: Response):
        try:
            return response.headers.get("request-id", None)
        except Exception as e:
            logger.error("Couldn't extract request ID: %s", e, exc_info=e)
        return None

    @staticmethod
    def _raise_unexpected_error(response: Response, err: Exception):
        logger.error(
            "Server response was: status: %s, response: %s",
            response.status_code,
            response.text,
        )
        logger.error(
            "Couldn't deserialize JSON error response: '%s'",
            response.text,
            exc_info=err,
        )
        raise RuntimeError("Couldn't parse JSON error response")

    @staticmethod
    def verify_raise_conditionally(response: Response):
        try:
            response.raise_for_status()
        except HTTPStatusError as err:
            try:
                # Try to deserialize error.
                error: RepositoryError = RepositoryError.parse_obj(response.json())
            except Exception as nested:
                RepositoryError._raise_unexpected_error(response, nested)

            request_id = RepositoryError._try_extract_request_id(response)
            if not isinstance(error.kind, RepositoryKindError):
                logger.error("Unhandled upstream error type: %s", error.kind)
            logger.error(
                "Something went sideways. request id: %s, code: %s, text: %s",
                request_id,
                response.status_code,
                response.text,
            )
            raise RepositoryException(error, request_id) from err

        except RepositoryException as err:
            raise err from err
        except Exception as err:
            RepositoryError._raise_unexpected_error(response, err)


class BaseRepositoryException(Exception):
    def __init__(self, error: RepositoryError, request_id: str):
        message = f"[{error.status_code}] [{request_id}] {error.kind}: {error.detail}"
        logger.error("error: %s", message)
        super().__init__(message)
        self.error = error
        self.request_id = request_id


class RepositoryException(BaseRepositoryException):
    pass
