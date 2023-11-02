import datetime
from enum import Enum
from pydantic import BaseModel, Field
from typing import Optional, Any, Annotated
from datetime import datetime
from repoclient.models.base_model import ClientBaseModel

from pydantic import (
    AfterValidator,
    PlainSerializer,
)

from repoclient.models.upload_session import QueryParamBase


class Column(BaseModel):
    column: str
    operator: Optional[str] = Field(None, alias="comparisonOperator")
    other: Optional[int | float | str | list] = Field(None, alias="compareAgainst")

    def _set(self, other: Any, operator: str):
        self.other = other
        if isinstance(other, datetime):
            self.other = other.isoformat()
        self.operator = operator
        return self

    @staticmethod
    def _assert_arg_supports_gt_lt_operators(arg: Any):
        assert isinstance(arg, (int, float, datetime)), f"{arg} is not numeric!"

    @staticmethod
    def _assert_arg_is_str(arg: Any):
        assert isinstance(arg, str), f"{arg} is not a string!"

    def matches_regex(self, other: str):
        self._assert_arg_is_str(other)
        return self._set(other, "regex")

    def matches_regex_case_insensitive(self, other: str):
        self._assert_arg_is_str(other)
        return self._set(other, "regexCaseInsensitive")

    def is_like_case_insensitive(self, other: str):
        self._assert_arg_is_str(other)
        return self._set(other, "iLike")

    def is_like(self, other: str):
        self._assert_arg_is_str(other)
        return self._set(other, "like")

    def is_in(self, other: list[int | float | str]):
        return self._set(other, "in")

    def __eq__(self, other: str | int | float | datetime):
        return self._set(other, "eq")

    def __gt__(self, other: int | float | datetime):
        self._assert_arg_supports_gt_lt_operators(other)
        return self._set(other, "gt")

    def __ge__(self, other: int | float | datetime):
        self._assert_arg_supports_gt_lt_operators(other)
        return self._set(other, "gte")

    def __lt__(self, other: int | float | datetime):
        self._assert_arg_supports_gt_lt_operators(other)
        return self._set(other, "lt")

    def __le__(self, other: int | float | datetime):
        self._assert_arg_supports_gt_lt_operators(other)
        return self._set(other, "lte")

    def __str__(self):
        return f"Column <'{self.column}' {self.operator} '{self.other}'>"


class QueryGroupKind(str, Enum):
    ALL = "all"
    ANY = "any"


class QueryGroup(BaseModel):
    kind: QueryGroupKind = Field(..., serialization_alias="conditionKind")
    is_not: Optional[bool] = Field(False, alias="not")
    args: list[Column]

    def negate(self):
        """Negate this query group.

        This will basically prepend a **NOT** <STATEMENT> when this query is sent
        to the server. Use negate() when you want data that doesn't match all (or any)
        of the statements in the group.

        Example::

            >>> group = QueryGroup(args=[Column(column="id") == 1])
            >>> group = group.negate()
            >>> group
            QueryGroup(args=[Column("id") == 1], kind=QueryGroupKind.ALL, is_not=True)

        :return: QueryGroup
        """
        self.is_not = True
        return self

    def match_any(self):
        """Return data that matches any statement in this group.

        This will basically prepend a **OR** <STATEMENT> when this query is sent
        to the server. Use any() when you want data that matches any of the statements
        in the group.

        Example::

            >>> group = QueryGroup(args=[Column(column="id") == 1])
            >>> group = group.match_any()
            >>> group
            QueryGroup(args=[Column(column="id") == 1], kind=QueryGroupKind.ANY)

        :return:
        """
        self.kind = QueryGroupKind.ANY
        return self


def _validate_class(o: object) -> QueryParamBase:
    assert isinstance(o, QueryParamBase), f"{o} is not a subclass of `QueryParamBase`!"
    return o


UploadSessionSerialized = Annotated[
    object,
    AfterValidator(_validate_class),
    PlainSerializer(lambda upload_session: upload_session.as_dict(), return_type=dict),
]


class Query(ClientBaseModel):
    format_id: Optional[list[int]] = Field(None, alias="formats")
    upload_session: Optional[UploadSessionSerialized] = Field(
        None, alias="uploadSession"
    )
    query: list[QueryGroup] = []

    @classmethod
    def new_empty(cls) -> "Query":
        """Create a new empty query. Usually useful to pull
        ALL data available for this user.

        :return: Query
        """
        return Query()
