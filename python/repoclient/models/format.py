from __future__ import annotations

import enum
from enum import Enum
from typing import Optional, Iterator, IO, Any
from io import IOBase, BytesIO
from pprint import pformat

import orjson.orjson
from pydantic import PrivateAttr, Field, field_serializer
from typing import Pattern
from datetime import datetime
from pandas import DataFrame, read_csv
from httpx import AsyncClient
import logging

from repoclient.exception import RepositoryError, RepositoryException
from repoclient.models.handler import RequestModel
from repoclient.models.query import Query
from repoclient.models.upload_session import (
    UploadSession,
    QueryParamBase,
    ComparisonValidator,
    ComparisonMethod,
)
from repoclient.models.user import User
from repoclient.pagination import PaginatedResponse

logger = logging.getLogger("repoclient")

UNIX_EPOCH_DATE = "1970-00-00T00:00:00.000000+00:00"

MAX_SUGGESTED_PAYLOAD_SIZE = 10 * (1024 * 1024)  # 10MiB
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
    DATETIME = "Datetime"


class ColumnSchema(RequestModel):
    name: str
    kind: ColumnKind
    regex: Optional[Pattern] = None

    @classmethod
    def numeric(cls, name: str):
        return cls(name=name, kind=ColumnKind.NUMBER)

    @field_serializer("regex")
    def serialize_dt(self, regex: Optional[Pattern], _info):
        if regex is None:
            return None
        return str(regex.pattern)

    @classmethod
    def string(cls, name: str, regex: Optional[Pattern] = None):
        return cls(name=name, kind=ColumnKind.STRING, regex=regex)

    @classmethod
    def datetime(cls, name: str):
        return cls(name=name, kind=ColumnKind.DATETIME)

    def get_python_type(self) -> str:
        # Return the pandas dtype of this column.
        if self.kind is ColumnKind.NUMBER:
            return float
        elif self.kind is ColumnKind.STRING:
            return str
        elif self.kind is ColumnKind.DATETIME:
            return "datetime64[ns, UTC]"
        raise RuntimeError("Unknown kind")


class FormatUploadSessionFilter(str, enum.Enum):
    """Upload Session filtering utilities.

    You can use any member in this enum to perform
    filtering on a specific column using `P` objects.
    Example:

        >>> from repoclient import P, Query
        >>> upload_session = P(FormatUploadSessionFilter.ID) == "SomeId"
        >>> query = Query(query=[], upload_session=upload_session)
        >>> # Now you can use this `query` to filter some data!

    """

    # Upload session ID
    ID = "id"
    # Records saved for this upload session
    RECORD_COUNT = "recordCount"
    # Filter by format ID (this can also be done
    # by passing `format_id` to `Query`.
    FORMAT_ID = "formatId"
    # Filter upload sessions made by this user.
    USER_ID = "userId"
    # Upload sessions with successful outcomes.
    OUTCOME = "outcome"
    # Upload sessions created at this time.
    CREATED_AT = "createdAt"


class FormatUploadSession(QueryParamBase):
    _ALLOWED_COLUMN_CLASS_ = FormatUploadSessionFilter

    _ALLOWED_FIELDS_ = {
        FormatUploadSessionFilter.ID: ComparisonValidator(
            int, ComparisonMethod.supports_all()
        ),
        FormatUploadSessionFilter.RECORD_COUNT: ComparisonValidator(
            int, ComparisonMethod.supports_all()
        ),
        FormatUploadSessionFilter.FORMAT_ID: ComparisonValidator(
            int, ComparisonMethod.supports_all()
        ),
        FormatUploadSessionFilter.USER_ID: ComparisonValidator(
            str, [ComparisonMethod.EQUAL]
        ),
        FormatUploadSessionFilter.OUTCOME: ComparisonValidator(
            str, [ComparisonMethod.EQUAL]
        ),
        FormatUploadSessionFilter.CREATED_AT: ComparisonValidator(
            datetime, ComparisonMethod.supports_all()
        ),
    }


class Format(RequestModel):
    id: Optional[int] = None
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
            FORMAT_URL, json=self.model_dump(by_alias=True), headers=user.bearer
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
        self, client: AsyncClient, user: User, query: Query = Query.new_empty()
    ) -> Iterator[Record]:
        """Get the count of existing items for any given query.
        If a query isn't passed, this will return the count of all
        the available items for the passed `user`.

        :param client: HTTP Client
        :param user: User
        :param query: Get count for this query
        :return:
        """

        assert self._checked, "Uninitialized format; call create or get first"
        json_query = query.model_dump(by_alias=True)
        return await PaginatedResponse.get_count(
            upstream=f"{RECORD_URL}/filter",
            client=client,
            user=user,
            json=json_query,
            method="POST",
        )

    async def get_stream_data_pandas_dangerous(
        self,
        client: AsyncClient,
        user: User,
        query: Query,
        chunk_size: int = 10 * (1024 * 1024),
        supress_warning: bool = False,
    ) -> DataFrame:
        """
        Get data from this format and write it to a CSV file.

        This method internally uses `get_data_csv_stream` to get the
        data and then converts it to a Pandas dataframe. This method
        might be a little bit faster than `get_data_pandas_dangerous()`.

        However, you won't be able to make multiple stream queries at
        the same time since this has a higher cost at the database level.
        This limitation is enforced server-side on a per-user basis.

        :param client: HTTP Client
        :param user: User
        :param query: Query
        :param chunk_size: Chunk size (default: 10MiB)
        :param supress_warning: Whether to supress warnings or not
        :return:
        """
        assert self._checked, "Uninitialized format; call create or get first"
        if not supress_warning:
            logger.warning(
                "This method can potentially load a lot of data into memory. "
                "Please use `get_data_csv_stream` to use your own IO buffer,"
                " or pass `supress_warning` to hide this warning."
            )

        buffer = BytesIO()
        await self.get_data_csv_stream(client, user, query, buffer, chunk_size)
        logger.debug(
            "Loading into dataframe and setting types for %s column(s)",
            len(self.schema_ref),
        )
        buffer.seek(0)
        df = read_csv(buffer, dtype=object)

        if df.empty is True:
            raise AssertionError(
                "No data received: ensure you have READ access "
                "on this format and a valid query."
            )

        for column in self.schema_ref:
            df[column.name] = df[column.name].astype(column.get_python_type())

        return df

    async def upload_from_dataframe(
        self,
        client: AsyncClient,
        user: User,
        df: DataFrame,
        cast_types=True,
        fill_na=False,
    ) -> list[UploadSession]:
        """Upload data from this dataframe into this format.

        :param client: HTTP Client
        :param user: User
        :param df: Pandas dataframe
        :param cast_types: Whether to enable explicit data typecasting
        :param fill_na: Whether to fill missing/empty values or not
        :return: A list of upload sessions
        """
        assert self._checked, "Uninitialized format; call create or get first"
        assert df.shape[1] == len(
            self.schema_ref
        ), "Dataframe has wrong number of columns"

        df_columns = set(list(df.columns))
        format_columns = set(col.name for col in self.schema_ref)
        assert (
            df_columns == format_columns
        ), "Dataframe and format have different columns."

        for column in df.columns:
            na_count = df[column].isna().sum()
            if na_count > 0:
                logger.warning("Column %s contains %s null values", column, na_count)
                assert fill_na is True, (
                    f"Column {column} contains {na_count} nulls and `fill_na` is "
                    " set to False. This will result in a failed upload, "
                    " refusing to continue."
                )

        if fill_na is True:
            logger.warning(
                "`fill_na` is enabled. `fill_na` will fill missing values "
                "with 0's or empty spaces depending on the column type. "
                " This might have collateral effects on the uploaded data."
            )

        for column in self.schema_ref:
            if fill_na is True:
                if column.kind is ColumnKind.STRING:
                    df[column.name].fillna(value="", inplace=True)
                elif column.kind is ColumnKind.NUMBER:
                    df[column.name].fillna(value=0, inplace=True)
                elif column.kind is ColumnKind.DATETIME:
                    df[column.name].fillna(value=UNIX_EPOCH_DATE, inplace=True)
                logger.warning("filled na for %s", column.name)

            if cast_types is True:
                df[column.name] = df[column.name].astype(column.get_python_type())
                if column.kind is ColumnKind.DATETIME:
                    # Convert timestamps to ISO format
                    df[column.name] = df[column.name].apply(
                        lambda x: x.isoformat() if x is not None else None
                    )
                logger.debug("casted %s to %s", column.name, column.get_python_type())

        logger.debug("Successfully applied dataframe transformations: \n%s", df)

        # Convert dataframe to list of dictionaries
        records: list[dict[str, Any]] = df.to_dict(orient="records")
        return await self.upload_data(client, user, records)

    async def get_data_pandas_dangerous(
        self, client: AsyncClient, user: User, query: Query, *args, **kwargs
    ) -> DataFrame:
        """Get all data from the repository in a pandas DataFrame.

        This function will use all the pagination features for that purpose,
        which means you can use a very aggressive `per_page` value.

        For documentation, please see the related function: `get_data`.
        This function accepts exactly the same arguments.

        WARNING:
        This method buffers all the results into a list before building the dataframe.
        If the passed query matches too many results this function might end up
        freezing your computer.
        """
        assert self._checked, "Uninitialized format; call create() or get() first"

        if query.format_id is None:
            logger.warning(NO_FORMAT_ID_WARN_MSG)
        logger.warning(
            "Using the `get_data_pandas` method is discouraged as this method"
            " needs to load all data into memory first. This might cause"
            " resource exhaustion when loading large datasets."
        )

        json_query = query.model_dump(by_alias=True)
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

    async def get_data_csv_stream(
        self,
        client: AsyncClient,
        user: User,
        query: Query,
        output: IO[bytes],
        chunk_size: int = 1024 * (1024 * 10),
    ):
        """Get all data from the repository, and save it to a IO-like file.

        Unlike all the other get_* functions, `get_data_csv_stream` doesn't return the
        raw data or a dataframe using pagination. This function pulls all
        the available data using only a single connection to a bytes-like
        object.


        WARNING: It's your responsibility to manage the passed buffer
        (`output`). This includes but is not limited to taking care of memory
        management and async/sync IO writes.

        :param client: HTTP Client
        :param user: Authenticated user
        :param query: Filers to use for this query
        :param output: Bytes-like writable object
        :param chunk_size: Buffer size. Default: 10 MiB
        """
        assert self._checked, "Uninitialized format; call create or get first"
        if query.format_id is None:
            logger.warning(NO_FORMAT_ID_WARN_MSG)

        assert isinstance(output, IOBase), "`output` isn't a bytes-like object"

        json_query = query.model_dump(by_alias=True)
        output.seek(0)

        start = datetime.now()
        read_bytes = 0
        logger.debug("json query:  %s", pformat(json_query))

        async with client.stream(
            "POST", f"{RECORD_URL}/filter-stream", json=json_query, headers=user.bearer
        ) as response:
            if response.status_code != 200:
                req_id = response.headers.get("request-id", "N/A")
                data = await response.aread()
                json = orjson.loads(data)
                error = RepositoryError.model_validate(json)
                raise RepositoryException(error, req_id)

            async for data in response.aiter_bytes(chunk_size=chunk_size):
                read_bytes += len(data)
                output.write(data)

        elapsed = datetime.now() - start
        elapsed = elapsed.total_seconds() + elapsed.microseconds / 1000000
        read_mebibyte = read_bytes / (1024 * 1024)
        logger.info(
            "csv stream: %.2f MiB, %.2f MiB/s avg (%s bytes in %.3fs)",
            read_mebibyte,
            read_mebibyte / elapsed,
            read_bytes,
            elapsed,
        )

    async def get_data(
        self, client: AsyncClient, user: User, query: Query, **kwargs
    ) -> Iterator[Record]:
        """Get all data from the repository, using pagination if necessary.

        Note that you can pass arbitrary kwargs; these keyword-only arguments will
        be relayed to the pagination function. This allows you to control
        things like the pagination strategy (parallel, fast, default) or items
        pulled per request. Currently, you can use the following kwargs:

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
        json_query = query.model_dump(by_alias=True)

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
        assert len(data) > 0, "`data` must not be an empty list!"
        assert all(
            isinstance(i, dict) for i in data
        ), "expected list of dicts, got something else"
        payload = {"formatId": int(self.id), "data": data}
        json = orjson.orjson.dumps(payload)
        payload_size = len(json)
        logger.debug("JSON payload size: %.2f MiB", payload_size / (1024 * 1024))

        if payload_size > MAX_SUGGESTED_PAYLOAD_SIZE:
            logger.warning(
                "Payload exceeds the suggested size (%s bytes > %s). "
                "Please consider uploading your data in smaller chunks.",
                payload_size,
                MAX_SUGGESTED_PAYLOAD_SIZE,
            )
        response = await client.post(RECORD_URL, json=payload, headers=user.bearer)
        RepositoryError.verify_raise_conditionally(response)
        return UploadSession.model_validate(response.json())
