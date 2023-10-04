from __future__ import annotations

from typing import Optional
from datetime import datetime
from pydantic import PrivateAttr, Field
from httpx import AsyncClient

from repoclient.exception import RepositoryError
from repoclient.models.handler import RequestModel


class User(RequestModel):
    username: str
    password: Optional[str]
    id: Optional[str] = None
    created_at: Optional[datetime] = Field(None, alias="createdAt")
    is_superuser: Optional[bool] = Field(False, alias="isSuperuser")
    active: Optional[bool] = None
    token: Optional[str] = None
    _checked: bool = PrivateAttr(False)

    @property
    def is_valid(self):
        return self._checked

    async def login(self, client: AsyncClient):
        """Authenticate with the user's credentials.

        :param client: HTTP Client
        :return: User
        """
        assert self.password is not None, "password isn't set!"
        response = await client.post("/login", json=self.dict())
        RepositoryError.verify_raise_conditionally(response)
        json = response.json()
        ret: User = User.parse_obj(json["user"])
        ret.token = json["token"]
        ret._checked = True
        return ret

    @classmethod
    async def get(cls, client: AsyncClient, user: User, id: int) -> User:
        response = await client.get(f"/user/{id}", headers=user.bearer)
        RepositoryError.verify_raise_conditionally(response)
        ret = User.parse_obj(response.json())
        ret._checked = True
        return ret

    @property
    def bearer(self) -> dict:
        """Get this user's auth credentials as a dictionary.

        :return: dict
        """
        assert (
            self._checked
        ), "user instance not initialized, please call login() or get()"
        return {"Authorization": f"Bearer {self.token}"}

    def __str__(self):
        return (
            f"User <username: {self.username}, checked: {self._checked}, id: {self.id}>"
        )

    async def create_user(self, client: AsyncClient, user: User) -> User:
        assert self.is_superuser, "only superusers may use this resource"
        response = await client.post(
            "/user",
            headers=self.bearer,
            json=user.dict(by_alias=True, exclude_none=True),
        )
        RepositoryError.verify_raise_conditionally(response)
        ret: User = User.parse_obj(response.json())
        # copy over the original user's password
        ret.password = user.password
        ret._checked = True
        return ret
