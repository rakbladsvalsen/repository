from __future__ import annotations

from typing import Optional
from pydantic import Field
from enum import Enum
from httpx import AsyncClient
import logging

from repoclient import User
from repoclient.exception import RepositoryError
from repoclient.models.handler import RequestModel

logger = logging.getLogger("repoclient")


class EntitlementAccess(str, Enum):
    READ_ONLY = "readOnly"
    READ_WRITE = "readWrite"
    WRITE_ONLY = "writeOnly"


class FormatEntitlement(RequestModel):
    user_id: int = Field(alias="userId")
    format_id: int = Field(alias="formatId")
    access: Optional[EntitlementAccess] = None

    async def create(self, client: AsyncClient, user: User) -> FormatEntitlement:
        # this is also enforced server-side
        assert user.is_superuser, "Only superusers may use this resource"
        assert self.access is not None, "access not set"
        response = await client.post(
            "/entitlement", headers=user.bearer, json=self.dict(by_alias=True)
        )
        RepositoryError.verify_raise_conditionally(response)
        return FormatEntitlement.parse_obj(response.json())
