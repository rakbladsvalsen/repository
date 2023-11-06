from __future__ import annotations


from typing import Optional, Iterator
from datetime import datetime

from pydantic import Field
from enum import Enum
from httpx import AsyncClient
import logging

from repoclient import User
from repoclient.exception import RepositoryError
from repoclient.models.common import UserFormatFilter
from repoclient.models.handler import RequestModel
from repoclient.pagination import PaginatedResponse

logger = logging.getLogger("repoclient")


class EntitlementAccessLevel(str, Enum):
    READ = "read"
    WRITE = "write"


class FormatEntitlement(RequestModel):
    user_id: str = Field(alias="userId")
    format_id: int = Field(alias="formatId")
    access: list[EntitlementAccessLevel]
    created_at: Optional[datetime] = Field(None, alias="createdAt")

    def __str__(self):
        return f"FormatEntitlement <user_id={self.user_id}, format_id={self.format_id}, access: {self.access}>"

    async def create(self, client: AsyncClient, user: User) -> FormatEntitlement:
        """Create a Format Entitlement.

        Example::

            entitlement = await FormatEntitlement(
                user_id=123, format_id=321, access=EntitlementAccessLevel.WRITE
            )

        Note that ``user_id`` and ``format_id`` must be valid IDs. Both the user and format must exist.

        :param client:
        :param user:
        :return:
        """
        # this is also enforced server-side
        assert self.access is not None, "access isn't set"
        response = await client.post(
            "/entitlement", headers=user.bearer, json=self.model_dump(by_alias=True)
        )
        RepositoryError.verify_raise_conditionally(response)
        return FormatEntitlement.model_validate(response.json())

    async def delete(self, client: AsyncClient, user: User):
        """Delete a format.

        Example::

            await FormatEntitlement(
                user_id=123, format_id=321
            ).delete()

        Note that this entitlement, otherwise an exception will be raised.

        :param client:
        :param user:
        """
        assert user.is_superuser, "Only superusers may use this resource"
        response = await client.request(
            "DELETE",
            "/entitlement",
            # no need to pass created_at
            json=self.model_dump(by_alias=True, exclude={"created_at"}),
            headers=user.bearer,
        )
        RepositoryError.verify_raise_conditionally(response)

        logger.debug(
            "successfully deleted entitlement for: user id: %s, on format id %s",
            self.user_id,
            self.format_id,
        )

    @staticmethod
    async def get_all(
        client: AsyncClient,
        user: User,
        **kwargs,
    ) -> Iterator[FormatEntitlement]:
        """Get all available format entitlements.

        :param client: HTTP Client
        :param user: Authenticated user
        :param query: Optional query to apply.
        :return: Async iterator
        """
        # upstream = FormatEntitlement._build_upstream_url_filtered(filters)
        upstream = "/entitlement"

        async for item in PaginatedResponse.get_all(
            upstream=upstream,
            klass=list[FormatEntitlement],
            client=client,
            user=user,
            **kwargs,
        ):
            for it in item:
                yield it
