from __future__ import annotations

from enum import Enum
from typing import Optional, Iterator
from datetime import datetime
from pydantic import PrivateAttr, Field
from httpx import AsyncClient
import logging
from repoclient.models.handler import RequestModel
from repoclient.models.query import Query
from repoclient.models.upload_session import UploadSession
from repoclient.models.user import User
from repoclient.pagination import PaginatedResponse

logger = logging.getLogger("repoclient")

FORMAT_URL = "/format"
RECORD_URL = "/record"
NO_FORMAT_ID_WARN_MSG = """\
You are querying the repository without specifying a format_id. \
This will return all records available for your user. This can \
significantly slow down your query. Please specify a format_id."""


class Record(RequestModel):
    id: int
    upload_session_id: int
    data: dict[str, int | float | str]


class ColumnKind(str, Enum):
    NUMBER = "Number"
    STRING = "String"


class ColumnSchema(RequestModel):
    name: str
    kind: ColumnKind

    @classmethod
    def numeric(cls, name: str):
        return cls(name=name, kind=ColumnKind.NUMBER)

    @classmethod
    def string(cls, name: str):
        return cls(name=name, kind=ColumnKind.STRING)

class Format(RequestModel):
    id: Optional[str]
    name: str
    description: str
    created_at: Optional[datetime] = None
    schema_ref: list[ColumnSchema] = Field(alias="schema")
    _checked: bool = PrivateAttr(False)

    def __str__(self):
        return f"Format <{self.name}, id: {self.id}, checked: {self._checked}>"

    @property
    def columns(self) -> list[ColumnSchema]:
        """Get this format's columns.

        :return: Column schema
        """
        return self.schema_ref

    @staticmethod
    async def get_all(
        client: AsyncClient, user: User, per_page: int = 1000
    ) -> Iterator[Format]:
        """Get all available formats.
        Note: superusers have complete visibility of all formats.

        Normal users can only see the formats for which they have
        an available entitlement.

        :param client:
        :param user:
        :param per_page:
        """
        proxy_handler = RequestModel()
        async for item in PaginatedResponse.get_all(
            upstream=FORMAT_URL,
            klass=Format,
            client=client,
            user=user,
            exc_handler=proxy_handler.handle_exception,
            per_page=per_page,
        ):
            item._checked = True
            yield item

    async def create(self, client: AsyncClient, user: User) -> Format:
        """Create the format. This call may only be used by superusers.

        :param client: HTTP Client
        :param user: Authenticated user
        :return: Format
        """
        response = await client.post(
            FORMAT_URL, json=self.dict(by_alias=True), headers=user.bearer
        )
        if response.status_code != 201:
            self.handle_exception(response)
        self.id = response.json()["id"]
        logger.debug("successfully created format, id: %s", self.id)
        self._checked = True
        return self

    @classmethod
    async def get(cls, client: AsyncClient, id: int, user: User) -> Format:
        """Get this format by ID.

        Note: Normal users (i.e. users without superuser flag) will need
        to have an entitlement to be able to pull this format.
        Superusers automatically bypass this restriction.

        :param client: HTTP Client
        :param id: Format ID
        :param user: Authenticated user
        :return:
        """
        response = await client.get(f"{FORMAT_URL}/{id}", headers=user.bearer)
        if response.status_code != 200:
            cls.handle_exception(response)
        json = response.json()
        ret = cls(**json)
        ret._checked = True
        return ret

    async def delete(self, client: AsyncClient, user: User):
        """Delete this format. Only superusers may use this call.

        :param client: HTTP Client
        :param user: Authenticated user
        :return None
        """
        assert self._checked, "Uninitialized format; call create or get first"
        response = await client.delete(f"{FORMAT_URL}/{self.id}", headers=user.bearer)
        if response.status_code != 204:
            self.handle_exception(response)
        logger.debug("successfully deleted format, id: %s", self.id)
        return True

    async def get_count(
        self, client: AsyncClient, user: User, query: Query
    ) -> Iterator[Record]:
        assert self._checked, "Uninitialized format; call create or get first"
        if query.format_id is None:
            logger.warning(NO_FORMAT_ID_WARN_MSG)
        json_query = query.dict(by_alias=True)
        return await PaginatedResponse.get_count(
            upstream=f"{RECORD_URL}/filter",
            client=client,
            user=user,
            exc_handler=self.handle_exception,
            json=json_query,
        )

    async def get_data(
        self, client: AsyncClient, user: User, query: Query, **kwargs
    ) -> Iterator[Record]:
        """Query the repository using the default strategy.

        Note that you can pass arbitrary kwargs; these keyword-only arguments will
        be relayed to the pagination function. This allows you to control
        things like the pagination strategy (parallel, fast, default) or items
        pulled per request.

        :param client: HTTP Client
        :param user: Authenticated user
        :param query: Filers to use for this query
        :return Async iterator
        """
        assert self._checked, "Uninitialized format; call create or get first"
        if query.format_id is None:
            logger.warning(NO_FORMAT_ID_WARN_MSG)
        json_query = query.dict(by_alias=True)

        async for item in PaginatedResponse.get_all(
            upstream=f"{RECORD_URL}/filter",
            klass=Record,
            client=client,
            user=user,
            exc_handler=self.handle_exception,
            json=json_query,
            **kwargs,
        ):
            yield item

    async def upload_data(
        self, client: AsyncClient, user: User, data: list[dict]
    ) -> UploadSession:
        """Upload data to this format.

        `data` must be a list of dicts containing the data to be uploaded.

        Each dictionary must contain the **EXACT** columns defined for this format,
        and the values must have the matching type as well, otherwise an
        `InvalidDataException` will be raised.

        :param client: HTTP Client
        :param user: Authenticated user with Read/ReadWrite access on this format
        :param data: Raw dict data
        :return: Upload session
        """
        assert self._checked, "Uninitialized format; call create or get first"
        assert isinstance(data, list), "`data` must be an array of dicts!"
        assert all(
            isinstance(i, dict) for i in data
        ), "expected list of dicts, got something else"
        payload = {"formatId": self.id, "data": data}
        response = await client.post(RECORD_URL, json=payload, headers=user.bearer)
        if response.status_code != 200:
            self.handle_exception(response)
        return UploadSession.parse_obj(response.json())