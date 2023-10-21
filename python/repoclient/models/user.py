from __future__ import annotations

from typing import Optional, Iterator
from datetime import datetime
from pydantic import PrivateAttr, Field
from httpx import AsyncClient

from repoclient.exception import RepositoryError
from repoclient.models.handler import RequestModel


class UserApiKey(RequestModel):
    user_id: str = Field(..., alias="userId")
    id: str
    created_at: datetime = Field(..., alias="createdAt")
    last_rotated_at: datetime = Field(..., alias="lastRotatedAt")
    active: bool
    _token: str = PrivateAttr(None)
    _parent_user: User = PrivateAttr(None)

    @property
    def has_token(self):
        return self._token is not None

    @property
    def token(self):
        assert self.has_token, "object doesn't have a token inside"
        return self._token

    async def delete_key(self, client: AsyncClient, user: User = None):
        """
        Delete an API key for user `user` using `user`'s token.
        This call is meant to be called by the user itself, i.e. the owner
        of the token. To delete someone else's keys, use `delete_for_user`
        instead.

        If this API key was created using one of the create methods,
        then `user` won't be needed as it'll use the parent user's token.

        :param client:
        :param user:
        :return:
        """
        if user is None and self._parent_user is None:
            raise AssertionError(
                "`user` was None and there's no stored instance "
                "of the user this api key belongs to. Please pass "
                " a valid `user` instance."
            )

        if user is None:
            user = self._parent_user

        await self.delete_for_user(client, user, user)

    async def delete_for_user(
        self, client: AsyncClient, caller: User, target_user: User
    ):
        """
        Delete an API key for `target_user`.
        If `caller` (the user that is invoking the API) is different
        from `target_user` (for which we're deleting the API key), and `caller`
        isn't an admin, an error will be raised. This is also enforced at the API level.

        :param client:
        :param caller:
        :param target_user:
        :return:
        """
        assert self._token is not None, "object isn't initialized"
        assert caller.id is not None, "`caller` isn't initialized"
        assert target_user.id is not None, "`target_user` isn't initialized"
        if caller.id != target_user.id:
            assert (
                caller.is_superuser
            ), "Normal users cannot delete keys for another user"

        response = await client.delete(
            f"/user/{target_user.id}/api-key/{self.id}", headers=caller.bearer
        )
        RepositoryError.verify_raise_conditionally(response)

    @classmethod
    async def create_for_user(
        cls, client: AsyncClient, caller: User, target_user: User
    ) -> "UserApiKey":
        """
        Create an API key for user `target_user`.
        If `caller` (the user that is invoking the API) is different
        from `target_user` (for which we're creating the API key), and `caller`
        isn't an admin, an error will be raised. This is also enforced at the API level.

        :param client:
        :param caller:
        :param target_user:
        :return:
        """
        assert caller.id is not None, "`caller` isn't initialized"
        assert target_user.id is not None, "`target_user` isn't initialized"
        if caller.id != target_user.id:
            assert (
                caller.is_superuser
            ), "Normal users cannot create keys for another user"

        response = await client.post(
            f"/user/{target_user.id}/api-key", headers=caller.bearer
        )
        RepositoryError.verify_raise_conditionally(response)
        json = response.json()
        api_key = json["apiKey"]
        ret: UserApiKey = UserApiKey.parse_obj(api_key)
        ret._token = json["token"]
        ret._parent_user = target_user
        return ret


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

    async def login(self, client: AsyncClient) -> User:
        """Authenticate with the user's credentials.

        :param client: HTTP Client
        :return: User
        """
        assert self.password is not None, "password isn't set!"
        response = await client.post("/login", json=self.dict())
        RepositoryError.verify_raise_conditionally(response)
        json = response.json()
        ret: User = User.parse_obj(json["user"])
        ret.id = json["user"]["id"]
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
            f"User <username: {self.username}, checked: {self._checked}, "
            f"id: {self.id}, checked: {self._checked}>"
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

    async def create_api_key(self, client: AsyncClient) -> UserApiKey:
        """Create an API key for this user.

        This user must've been initialized first.

        :param client:
        :return:
        """
        assert (
            self._checked
        ), f"user not initialized: call create_user(), get() or login() first"
        return await UserApiKey.create_for_user(client, self, self)

    async def delete_user(self, client: AsyncClient, user: User) -> User:
        """

        :param client: HTTP Client
        :param user: Target user to delete
        :return: None
        """
        assert self.is_superuser, "only superusers may use this resource"
        assert user.id is not None, f"{user}: user is not initialized"

        response = await client.delete(
            f"/user/{user.id}",
            headers=self.bearer,
        )
        RepositoryError.verify_raise_conditionally(response)

    async def get_all_keys(
        self, client: AsyncClient, user: User = None, per_page: int = 1000
    ) -> Iterator[UserApiKey]:
        from repoclient import PaginatedResponse

        """Get all available api keys for user `user`.

        :param client:
        :param user:
        :param per_page:
        """
        if self is not None:
            user = self

        async for item in PaginatedResponse.get_all(
            upstream="/user/api-key",
            klass=list[UserApiKey],
            client=client,
            user=user,
            per_page=per_page,
        ):
            for it in item:
                it: UserApiKey
                it._parent_user = user
                yield it
