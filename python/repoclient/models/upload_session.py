from __future__ import annotations

import enum
from enum import Enum

from httpx import AsyncClient
from pydantic import Field, BaseModel, UUID4
from typing import Optional, Iterator, TypeVar, Tuple, Type
from datetime import datetime
import logging


from repoclient.models.user import User
from repoclient.pagination import PaginatedResponse

logger = logging.getLogger("repoclient")

T = TypeVar("T")


class P:
    """
    'Parameter' class.
    This class is meant to be exclusively with query paramters.

    Example:
        >>> param = P("field")
        # Compare "field" greater than 5
        >>> param > 5
        #  Compare "field" less than datetime.now()
        >>> param < datetime.now()
        #  Compare "field" greater than or equal to 5
        >>> param >= 5

    The following comparison operators are supported:

    - >
    - >=
    - <
    - <=
    - ==

    NOT comparisons (~) aren't supported.
    """

    def __init__(self, field: enum.Enum):
        self._field = field
        self._other: Optional[T] = None
        self._cmp_op: Optional[ComparisonMethod] = None

    def __str__(self):
        return f"P<{self._other}>"

    @property
    def field_type(self) -> Type:
        return type(self._field)

    @property
    def is_comparison_initialized(self) -> bool:
        return self._cmp_op is not None and self._other is not None

    @property
    def field(self) -> str:
        return self._field.value

    @property
    def other(self) -> T:
        assert self.is_comparison_initialized, "Comparison method must be initialized"
        return self._other

    @property
    def comparison_operator(self) -> ComparisonMethod:
        assert self.is_comparison_initialized, "Comparison method must be initialized"
        return self._cmp_op

    def __gt__(self, other) -> P:
        self._cmp_op = ComparisonMethod.GREATER_THAN
        self._other = other
        return self

    def __ge__(self, other) -> P:
        self._cmp_op = ComparisonMethod.GREATER_THAN_EQUAL
        self._other = other
        return self

    def __lt__(self, other) -> P:
        self._cmp_op = ComparisonMethod.LESS_THAN
        self._other = other
        return self

    def __le__(self, other) -> P:
        self._cmp_op = ComparisonMethod.LESS_THAN_OR_EQUAL
        self._other = other
        return self

    def __eq__(self, other) -> P:
        self._cmp_op = ComparisonMethod.EQUAL
        self._other = other
        return self


class ComparisonValidator:
    def __init__(
        self,
        field_type: type,
        allowed_comparison_methods: list[ComparisonMethod],
    ):
        assert (
            len(allowed_comparison_methods) > 0
        ), "At least one comparison method must be provided"
        self.field_type = field_type
        self.allowed_comparison_methods = allowed_comparison_methods

    def _verify_inner_types(self, param: P):
        other: T = param.other
        cmp_op: ComparisonMethod = param.comparison_operator
        assert cmp_op in self.allowed_comparison_methods, f"{cmp_op} is not allowed"
        if self.field_type == str:
            assert isinstance(
                other, str
            ), "Only string comparison methods are supported for string fields"
        elif self.field_type == int:
            assert isinstance(
                other, int
            ), "Only integer comparison methods are supported for integer fields"
        elif self.field_type == datetime:
            assert isinstance(
                other, datetime
            ), "Only datetime comparison methods are supported for datetime fields"
        else:
            raise NotImplementedError(
                f"Comparison method for {type(self.field_type)} is not implemented"
            )

    def create_compare_statement(self, param: P) -> Tuple[str, str]:
        self._verify_inner_types(param)

        other = param.other

        if isinstance(other, datetime):
            other = other.isoformat()

        return (
            f"{param.field}{param.comparison_operator.value}",
            other,
        )


class ComparisonMethod(Enum):
    EQUAL = "Eq"
    GREATER_THAN = "Gt"
    GREATER_THAN_EQUAL = "Gte"
    LESS_THAN = "Lt"
    LESS_THAN_OR_EQUAL = "Lte"

    @staticmethod
    def supports_all() -> list[ComparisonMethod]:
        return [
            ComparisonMethod.EQUAL,
            ComparisonMethod.GREATER_THAN,
            ComparisonMethod.GREATER_THAN_EQUAL,
            ComparisonMethod.LESS_THAN,
            ComparisonMethod.LESS_THAN_OR_EQUAL,
        ]


class QueryParamBase:
    _ALLOWED_COLUMN_CLASS_: Type = None
    _ALLOWED_FIELDS_: dict[Type, ComparisonValidator] = {}

    def __init__(self, args: list[P]):
        assert (
            self._ALLOWED_COLUMN_CLASS_ is not None
        ), "Programming error: _ALLOWED_COLUMN_CLASS_ must be overridden"

        self._prepared_args: list[Tuple[str, str | int | datetime]] = []

        assert isinstance(args, list), "args must be a list"
        assert len(args) > 0, "At least one parameter must be provided"

        for arg in args:
            assert isinstance(arg, P), "Only P objects are supported"
            assert (
                arg.is_comparison_initialized
            ), "P objects must be compared against something else"
            assert arg.field_type == self._ALLOWED_COLUMN_CLASS_, (
                f"Expected {self._ALLOWED_COLUMN_CLASS_} but got {arg.field_type}."
                " You're probably using the wrong class for this query."
            )
            assert (
                arg.field in self._ALLOWED_FIELDS_
            ), f"{arg.field} not in {self._ALLOWED_FIELDS_.keys()}"
            validator = self._ALLOWED_FIELDS_[arg.field]
            self._prepared_args.append(validator.create_compare_statement(arg))

    @property
    def prepared_args(self) -> list[Tuple[str, str | int | datetime]]:
        assert len(self._prepared_args) > 0, "No prepared args found"
        return self._prepared_args

    def as_dict(self) -> dict[str, str | int | datetime]:
        return {attr: value for (attr, value) in self.prepared_args}

    def as_url_params(self) -> str:
        """
        Return this Query param's attributes as URL query
        parameters.

        :return:
        """
        buff = ""
        for attr, value in self.prepared_args:
            buff += f"{attr}={value}&"
        # Remove trailing "&"
        return buff[:-1]


class UploadSession(BaseModel):
    id: int
    created_at: datetime = Field(alias="createdAt")
    record_count: int = Field(alias="recordCount")
    format_id: int = Field(alias="formatId")
    user_id: UUID4 = Field(alias="userId")
    outcome: str
    detail: str

    @staticmethod
    async def get_all(
        client: AsyncClient,
        user: User,
        **kwargs,
    ) -> Iterator[UploadSession]:
        """Get all available format upload sessions.

        :param client: HTTP Client
        :param user: Authenticated user
        :return: Async iterator
        """
        upstream = "/upload_session?"

        async for item in PaginatedResponse.get_all(
            upstream=upstream,
            klass=list[UploadSession],
            client=client,
            user=user,
            **kwargs,
        ):
            for it in item:
                yield it
