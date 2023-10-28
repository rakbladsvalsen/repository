import repoclient
import pytest
import os

from .util import get_random_string, api_client, admin_user, normal_user

ADMIN_USERNAME = os.environ.get("ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("ADMIN_PASSWORD", "admin")
SERVER_MAX_API_KEYS: int = 10


@pytest.mark.asyncio
async def test_login_admin(api_client):
    # test admin login
    user = await repoclient.User(
        username=ADMIN_USERNAME, password=ADMIN_PASSWORD
    ).login(api_client)
    assert user.token is not None, "token is none"
    assert user.is_valid, "user is not valid"


@pytest.mark.asyncio
async def test_new_user(api_client, admin_user):
    # create new user
    new_user = repoclient.User(
        username="test_" + get_random_string(20), password="random"
    )
    new_user = await admin_user.create_user(api_client, new_user)
    # check this new user is valid
    assert new_user.is_valid, "user is not valid"
    new_user_by_id = await repoclient.User.get(api_client, admin_user, new_user.id)
    assert new_user_by_id.is_valid, "user is not valid"
    # check this user can log in successfully
    new_user = await repoclient.User(
        username=new_user.username, password="random"
    ).login(api_client)
    assert new_user.is_valid, "user is not valid"
    assert new_user.is_superuser is False  # should not be a superuser
    assert new_user.token is not None, "token is none"
    api_key = await new_user.create_api_key(api_client)
    assert api_key.token is not None, "api key failure"
    await admin_user.delete_user(api_client, new_user)


async def test_max_api_keys_per_user(api_client, admin_user, normal_user):
    assert normal_user.is_valid, "user is not valid"
    for _ in range(SERVER_MAX_API_KEYS):
        api_key = await normal_user.create_api_key(api_client)
        assert api_key.token is not None, "api key failure"
    with pytest.raises(repoclient.RepositoryException) as exc:
        _api_key = await normal_user.create_api_key(api_client)
    assert exc is not None


async def test_get_all_keys_for_user(api_client, admin_user, normal_user):
    assert normal_user.is_valid, "user is not valid"
    seen_keys = set()
    for _ in range(SERVER_MAX_API_KEYS):
        api_key = await normal_user.create_api_key(api_client)
        seen_keys.add(api_key.id)
        assert api_key.token is not None, "api key failure"
    key_count = 0

    fetch_keys = set()
    async for key in normal_user.get_all_keys(api_client):
        assert key.has_token is False, "token shouldn't be initialized"
        key_count += 1
        fetch_keys.add(key.id)
    assert fetch_keys == seen_keys
    assert key_count == SERVER_MAX_API_KEYS


async def test_create_delete_repeatedly(api_client, admin_user, normal_user):
    assert normal_user.is_valid, "user is not valid"
    # api key creation should never fail because we always
    # delete the created key.
    for _ in range(SERVER_MAX_API_KEYS + 1):
        api_key = await normal_user.create_api_key(api_client)
        assert api_key.token is not None, "api key failure"
        await api_key.delete_key(api_client)
