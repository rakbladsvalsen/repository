from __future__ import annotations
import logging
from enum import Enum

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


class RepositoryError(BaseModel):
    status_code: int = Field(None, alias="statusCode")
    kind: RepositoryKindError | str
    detail: str


class BaseRepositoryException(Exception):
    def __init__(self, error: RepositoryError, request_id: str):
        message = f"[{error.status_code}] [{request_id}] {error.kind}: {error.detail}"
        logger.error("error: %s", message)
        super().__init__(message)
        self.error = error
        self.request_id = request_id


class RepositoryException(BaseRepositoryException):
    pass
