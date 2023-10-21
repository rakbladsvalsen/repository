import string
from io import BytesIO
from random import choice
import logging
import os

import pandas
import pytest
from httpx import AsyncClient
from pandas import read_csv

import repoclient
from repoclient import ColumnSchema, Query, User, Format

ADMIN_USERNAME = os.environ.get("ADMIN_USERNAME", "admin")
ADMIN_PASSWORD = os.environ.get("ADMIN_PASSWORD", "admin")

ENABLE_DEBUG_LOGS = bool(os.environ.get("ENABLE_DEBUG_LOGS", False))
REPOSITORY_URL = os.environ.get("REPOSITORY_URL", "http://localhost:8000")
REPOSITORY_TIMEOUT = int(os.environ.get("REPOSITORY_TIMEOUT", 60))

if ENABLE_DEBUG_LOGS is True:
    logging.getLogger("repoclient").setLevel(logging.DEBUG)


def get_random_string(length):
    # choose from all lowercase letter
    letters = string.ascii_lowercase
    return "".join(choice(letters) for i in range(length))


@pytest.fixture
async def api_client():
    async with AsyncClient(
        base_url=REPOSITORY_URL, timeout=REPOSITORY_TIMEOUT
    ) as api_client:
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
    new_user = await new_user.login(api_client)
    try:
        yield new_user
    except Exception as _:
        pass
    finally:
        await admin_user.delete_user(api_client, new_user)


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


async def load_streaming_query_into_df(
    api_client: AsyncClient, admin_user: User, fmt: Format, query: Query
) -> pandas.DataFrame:
    """
    Load a format query into a dataframe.
    This function uses the `get_data_csv_stream` utility to load all data into
    memory.

    :param api_client: Async HTTP Client
    :param admin_user: User
    :param fmt: Format
    :param query: Query
    :return: pandas.DataFrame
    """
    buffer = BytesIO()
    await fmt.get_data_csv_stream(api_client, admin_user, query, buffer)
    buffer.seek(0)
    return read_csv(buffer)
