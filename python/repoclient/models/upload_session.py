from __future__ import annotations
from httpx import AsyncClient
from pydantic import Field, BaseModel
from typing import Optional, Literal, Iterator
from datetime import datetime

from repoclient.models.user import User
from repoclient.pagination import PaginatedResponse
from repoclient.models.common import UserFormatFilter
from repoclient.models.handler import RequestModel


class UploadSessionQuery(UserFormatFilter):
    record_count_eq: Optional[int] = Field(None, alias="recordCountEq")
    record_count_gte: Optional[int] = Field(None, alias="recordCountGte")
    record_count_lte: Optional[int] = Field(None, alias="recordCountLte")
    outcome_eq: Optional[Literal["Success", "Error"]] = Field(
        "Success", alias="outcomeEq"
    )

    def record_count_equals(self, count: int):
        self.record_count_eq = count
        return self

    def record_count_less_than(self, count: int):
        self.record_count_lte = count
        return self

    def record_count_greater_than(self, count: int):
        self.record_count_gte = count
        return self

    def with_successful_outcome(self):
        self.outcome_eq = "Success"
        return self

    def with_error_outcome(self):
        self.outcome_eq = "Error"
        return self


class UploadSession(BaseModel):
    id: int
    created_at: datetime = Field(alias="createdAt")
    record_count: int = Field(alias="recordCount")
    format_id: int = Field(alias="formatId")
    user_id: int = Field(alias="userId")
    outcome: str
    detail: str

    def __str__(self):
        return f"UploadSession <id: {self.id}, records: {self.record_count}, outcome: {self.outcome}>"

    @staticmethod
    async def get_all(
        client: AsyncClient,
        user: User,
        query: Optional[UploadSessionQuery] = None,
        **kwargs,
    ) -> Iterator[UploadSession]:
        """Get all available format upload sessions.

        :param client: HTTP Client
        :param user: Authenticated user
        :param query: Optional query to apply.
        :return: Async iterator
        """
        upstream = "/upload_session?"
        if query is not None:
            upstream += query.build_url(upstream)

        proxy_handler = RequestModel()
        async for item in PaginatedResponse.get_all(
            upstream=upstream,
            klass=UploadSession,
            client=client,
            user=user,
            exc_handler=proxy_handler.handle_exception,
            **kwargs,
        ):
            yield item
