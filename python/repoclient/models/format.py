from __future__ import annotations

from enum import Enum
from typing import Optional, Iterator
from datetime import datetime
from pydantic import PrivateAttr, Field
from httpx import AsyncClient
import logging

from repoclient.exception import RepositoryError
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

    def get_python_type(self) -> str:
        # Return the pandas dtype of this column.
        if self.kind is ColumnKind.NUMBER:
            return float
        elif self.kind is ColumnKind.STRING:
            return str
        raise RuntimeError("Unknown kind")


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
        async for item in PaginatedResponse.get_all(
            upstream=FORMAT_URL,
            klass=list[Format],
            client=client,
            user=user,
            per_page=per_page,
        ):
            for it in item:
                it._checked = True
                yield it

    async def create(self, client: AsyncClient, user: User) -> Format:
        """Create the format. This call may only be used by superusers.

        :param client: HTTP Client
        :param user: Authenticated user
        :return: Format
        """
        response = await client.post(
            FORMAT_URL, json=self.dict(by_alias=True), headers=user.bearer
        )
        RepositoryError.verify_raise_conditionally(response)
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
        RepositoryError.verify_raise_conditionally(response)
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
        RepositoryError.verify_raise_conditionally(response)
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
            json=json_query,
            method="POST",
        )

    async def get_data_pandas_dangerous(
        self, client: AsyncClient, user: User, query: Query, *args, **kwargs
    ) -> "pandas.DataFrame":
        """Get all data from the repository in a pandas DataFrame.

        For documentation, please see the related function: `get_data`.
        This function accepts exactly the same arguments.

        WARNING:
        This method buffers all the results into a list before building the dataframe.
        If the passed query matches too many results this function might end up
        freezing your computer.
        """

        try:
            from pandas import DataFrame
        except Exception as e:
            logger.error("Couldn't import pandas: ", exc_info=e)
            raise AssertionError("Please make sure pandas is installed")

        assert self._checked, "Uninitialized format; call create() or get() first"

        if query.format_id is None:
            logger.warning(NO_FORMAT_ID_WARN_MSG)
        logger.warning(
            "Using the `get_data_pandas` method is discouraged as this method"
            " needs to load all data into memory first. This might cause"
            " resource exhaustion when loading large datasets."
        )

        json_query = query.dict(by_alias=True)
        buffer: list[Record] = []

        async for items in PaginatedResponse.get_all(
            upstream=f"{RECORD_URL}/filter",
            klass=list[Record],
            client=client,
            user=user,
            json=json_query,
            method="POST",
            *args,
            **kwargs,
        ):
            for it in items:
                buffer.append(it.data)

        if len(buffer) > 0:
            df = DataFrame(buffer, dtype=object)
        else:
            # If there are no records for this format, still create a
            # dataframe with the right columns and types.
            logger.warning("Got empty buffer. The returned dataframe will be EMPTY.")
            empty_buff: dict[str, list] = {}
            for column in self.schema_ref:
                empty_buff[column.name] = []
            df = DataFrame(empty_buff, dtype=object)

        # Cast dataframe columns to proper types
        logger.debug(
            "Setting right column types for %s column(s)", len(self.schema_ref)
        )

        for column in self.schema_ref:
            df[column.name] = df[column.name].astype(column.get_python_type())

        return df

    async def get_data(
        self, client: AsyncClient, user: User, query: Query, **kwargs
    ) -> Iterator[Record]:
        """Get all data from the repository, using pagination if necessary.

        Note that you can pass arbitrary kwargs; these keyword-only arguments will
        be relayed to the pagination function. This allows you to control
        things like the pagination strategy (parallel, fast, default) or items
        pulled per request. Currently you can use the following kwargs:

        - per_page: int: Pull this many items per request
        - pagination_strategy: Use this `PaginationStrategy` to fetch items
        - max_concurrency: Controls the maximum amount of in-flight concurrent
        requests at any given moment.

        :param client: HTTP Client
        :param user: Authenticated user
        :param query: Filers to use for this query
        :return Async iterator
        """
        assert self._checked, "Uninitialized format; call create or get first"
        if query.format_id is None:
            logger.warning(NO_FORMAT_ID_WARN_MSG)
        json_query = query.dict(by_alias=True)

        async for items in PaginatedResponse.get_all(
            upstream=f"{RECORD_URL}/filter",
            klass=list[Record],
            client=client,
            user=user,
            json=json_query,
            method="POST",
            **kwargs,
        ):
            for it in items:
                yield it

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
        payload = {"formatId": int(self.id), "data": data}
        response = await client.post(RECORD_URL, json=payload, headers=user.bearer)
        RepositoryError.verify_raise_conditionally(response)
        return UploadSession.parse_obj(response.json())
