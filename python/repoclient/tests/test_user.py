import repoclient
import pytest
import logging
import os

from .util import get_random_string, api_client, admin_user

ADMIN_USERNAME = os.environ.get("ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("ADMIN_PASSWORD", "admin")


logging.getLogger("repoclient").setLevel(logging.DEBUG)


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
