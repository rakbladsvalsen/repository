import string
from random import choice
import logging
import os

import pytest
from httpx import AsyncClient

import repoclient
from repoclient import ColumnSchema

ADMIN_USERNAME = os.environ.get("ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("ADMIN_PASSWORD", "admin")


logging.getLogger("repoclient").setLevel(logging.DEBUG)


def get_random_string(length):
    # choose from all lowercase letter
    letters = string.ascii_lowercase
    return "".join(choice(letters) for i in range(length))


@pytest.fixture
async def api_client():
    async with AsyncClient(base_url="http://localhost:8000", timeout=60) as api_client:
        try:
            yield api_client
        except Exception as e:
            pass


@pytest.fixture
@pytest.mark.asyncio
async def admin_user(api_client):
    try:
        yield await repoclient.User(
            username=ADMIN_USERNAME, password=ADMIN_PASSWORD
        ).login(api_client)
    except Exception as e:
        pass


@pytest.fixture
@pytest.mark.asyncio
async def normal_user(api_client, admin_user):
    new_user = repoclient.User(
        username="test_" + get_random_string(20), password="random"
    )
    await admin_user.create_user(api_client, new_user)
    try:
        yield await new_user.login(api_client)
    except Exception as e:
        pass


@pytest.fixture
@pytest.mark.asyncio
async def sample_format(api_client, admin_user):
    numeric_column = ColumnSchema.numeric("NumericColumn")
    string_column = ColumnSchema.string("StringColumn")
    name = get_random_string(12)
    fmt = await repoclient.Format(
        name=name,
        description="some nice description",
        schema=[numeric_column, string_column],
    ).create(api_client, admin_user)
    assert fmt.id is not None, "format id is null"
    try:
        yield fmt
    except Exception as e:
        pass
    await fmt.delete(api_client, admin_user)
