import operator

import repoclient
import pytest

from repoclient import P, FormatUploadSession, FormatUploadSessionFilter

from .util import (
    get_random_string,
    api_client,
    admin_user,
    sample_format,
    normal_user,
    load_streaming_query_into_df,
)

RECORD_PAYLOAD_SIZES: list[int] = [
    2,
    10,
    20,
    50,
    100,
    200,
    500,
    1_000,
    2_000,
    5_000,
    8_000,
    9_000,
    10_000,
]


@pytest.mark.asyncio
async def test_upload_record_admin(
    api_client, admin_user, sample_format: repoclient.Format
):
    data = [{"NumericColumn": 123, "StringColumn": "abcdeasf"}] * 100
    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == 100
    # make sure record count is correct
    query = repoclient.Query(query=[], format_id=[sample_format.id])
    count = await sample_format.get_count(api_client, admin_user, query)
    assert count == 100, "wrong record count"


@pytest.mark.asyncio
async def test_upload_record_admin_wrong_type(
    api_client, admin_user, sample_format: repoclient.Format
):
    data = [{"NumericColumn": "123", "StringColumn": 123}] * 10
    with pytest.raises(repoclient.RepositoryException) as exc:
        _upload = await sample_format.upload_data(api_client, admin_user, data)
        # user cannot upload because they have insufficient perms
    exc: repoclient.RepositoryException = exc.value
    assert exc.request_id is not None
    assert exc.error.kind == repoclient.RepositoryKindError.VALIDATION_FAILURE


@pytest.mark.asyncio
async def test_upload_record_no_entitlement(
    api_client, normal_user: repoclient.User, sample_format: repoclient.Format
):
    data = [{"NumericColumn": 123, "StringColumn": "abcdeasf"}] * 100
    with pytest.raises(repoclient.RepositoryException) as exc:
        # user cannot upload because they have insufficient perms
        _upload = await sample_format.upload_data(api_client, normal_user, data)
    exc: repoclient.RepositoryException = exc.value
    assert exc.request_id is not None
    assert exc.error.kind == repoclient.RepositoryKindError.INSUFFICIENT_PERM


@pytest.mark.asyncio
async def test_entitlement_read_only(
    api_client,
    admin_user: repoclient.User,
    normal_user: repoclient.User,
    sample_format: repoclient.Format,
):
    data = [{"NumericColumn": 123, "StringColumn": "abcdeasf"}] * 100
    entitlement = await repoclient.FormatEntitlement(
        user_id=normal_user.id,
        format_id=sample_format.id,
        access=repoclient.EntitlementAccessLevel.READ_ONLY,
    ).create(api_client, admin_user)

    with pytest.raises(repoclient.RepositoryException) as exc:
        # user cannot upload because they have insufficient perms
        # even with write permissions.
        await sample_format.upload_data(api_client, normal_user, data)
    # delete this entitlement
    await entitlement.delete(api_client, admin_user)
    exc: repoclient.RepositoryException = exc.value
    assert exc.request_id is not None
    assert exc.error.kind == repoclient.RepositoryKindError.INSUFFICIENT_PERM


async def test_entitlement_read_write(
    api_client,
    admin_user: repoclient.User,
    normal_user: repoclient.User,
    sample_format: repoclient.Format,
):
    data = [{"NumericColumn": 123, "StringColumn": "abcdeasf"}] * 100
    entitlement = await repoclient.FormatEntitlement(
        user_id=normal_user.id,
        format_id=sample_format.id,
        access=repoclient.EntitlementAccessLevel.READ_WRITE,
    ).create(api_client, admin_user)
    # user can write (rw perms)
    upload_session = await sample_format.upload_data(api_client, normal_user, data)
    assert upload_session.record_count == 100, "wrong record count"
    # delete this entitlement
    await entitlement.delete(api_client, admin_user)


async def test_entitlement_write_only(
    api_client,
    admin_user: repoclient.User,
    normal_user: repoclient.User,
    sample_format: repoclient.Format,
):
    data = [{"NumericColumn": 123, "StringColumn": "abcdeasf"}] * 100
    entitlement = await repoclient.FormatEntitlement(
        user_id=normal_user.id,
        format_id=sample_format.id,
        access=repoclient.EntitlementAccessLevel.WRITE_ONLY,
    ).create(api_client, admin_user)
    # user can write (rw perms)
    upload_session = await sample_format.upload_data(api_client, normal_user, data)
    assert upload_session.record_count == 100, "wrong record count"
    # delete this entitlement
    await entitlement.delete(api_client, admin_user)


async def test_entitlement_create_normal_user(
    api_client, normal_user: repoclient.User, sample_format: repoclient.Format
):
    with pytest.raises(repoclient.RepositoryException) as exc:
        await repoclient.FormatEntitlement(
            user_id=normal_user.id,
            format_id=sample_format.id,
            access=repoclient.EntitlementAccessLevel.WRITE_ONLY,
        ).create(api_client, normal_user)

    # can't create stuff without admin perms
    exc: repoclient.RepositoryException = exc.value
    assert exc.request_id is not None
    assert exc.error.kind == repoclient.RepositoryKindError.ADMIN_ONLY


@pytest.mark.parametrize(
    "operator_expect_second_set",
    [
        # Operator | Whether the second upload set is expected to be received
        (operator.ge, True),  # Greater or equal to (Gte)
        (operator.lt, False),  # Less than (Lt)
    ],
)
@pytest.mark.parametrize("payload_size", [100])
@pytest.mark.asyncio
async def test_query_upload_session_created_at(
    api_client,
    admin_user,
    sample_format: repoclient.Format,
    payload_size: int,
    operator_expect_second_set: tuple[operator, bool],
):
    operator, expect_second_set = operator_expect_second_set
    # First upload session
    data = []
    for i in range(0, payload_size):
        entry = {"NumericColumn": i, "StringColumn": f"FirstSet"}
        data.append(entry)
    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == payload_size

    # Second upload session
    # Note that this upload session's NumericColumn starts
    # just where the last upload session ends.
    data = []
    for i in range(payload_size, payload_size * 2):
        entry = {"NumericColumn": i, "StringColumn": f"SecondSet"}
        data.append(entry)
    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == payload_size

    upload_session = FormatUploadSession(
        [operator(P(FormatUploadSessionFilter.CREATED_AT), upload.created_at)]
    )
    query = repoclient.Query(
        query=[], format_id=[sample_format.id], upload_session=upload_session
    )

    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_records = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            seen_records.add(item.data["NumericColumn"])

        assert len(seen_records) == payload_size
        if expect_second_set is True:
            assert min(seen_records) == payload_size
            assert max(seen_records) == (payload_size * 2) - 1
        else:
            assert min(seen_records) == 0
            assert max(seen_records) == payload_size - 1

    # Also check dataframe output
    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    dataframe["NumericColumn"] = dataframe["NumericColumn"].astype("float")

    unique_values = len(list(dataframe["NumericColumn"].unique()))
    assert unique_values == payload_size
    if expect_second_set is True:
        assert dataframe["NumericColumn"].min() == payload_size, "Min check failed"
        assert (
            dataframe["NumericColumn"].max() == (payload_size * 2) - 1
        ), "Max check failed"
        assert len(dataframe.index) == payload_size, "Dataframe has wrong dimensions"
        assert {"NumericColumn", "StringColumn"}.issubset(
            set(dataframe.columns)
        ), "CSV contains unexpected columns"


@pytest.mark.parametrize("payload_size", RECORD_PAYLOAD_SIZES)
@pytest.mark.asyncio
async def test_query(
    api_client, admin_user, sample_format: repoclient.Format, payload_size: int
):
    data = []
    string_data = []
    for i in range(0, payload_size):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        string_data.append(entry["StringColumn"])
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == payload_size
    group = repoclient.QueryGroup(
        args=[
            # query all numbers that are greater or equal than 50
            repoclient.Column(column="NumericColumn") >= payload_size // 2,
            # also query all strings that are in the string_data (basically the same condition as above)
            repoclient.Column(column="StringColumn").is_in(string_data),
        ]
    )
    query = repoclient.Query(query=[group], format_id=[sample_format.id])

    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_records = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            assert item.data["NumericColumn"] >= payload_size // 2
            seen_records.add(item.data["NumericColumn"])
        # make sure we have exactly 50 records
        assert len(seen_records) == payload_size // 2

    # Also check dataframe output
    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    dataframe["NumericColumn"] = dataframe["NumericColumn"].astype("float")

    unique_values = len(list(dataframe["NumericColumn"].unique()))
    assert unique_values == (
        payload_size // 2
    ), f"Expected {payload_size} unique values"
    assert dataframe["NumericColumn"].min() == payload_size // 2, "Min check failed"
    assert dataframe["NumericColumn"].max() == payload_size - 1, "Max check failed"
    assert len(dataframe.index) == (payload_size // 2), "Dataframe has wrong dimensions"
    assert {"NumericColumn", "StringColumn"}.issubset(
        set(dataframe.columns)
    ), "CSV contains unexpected columns"


@pytest.mark.parametrize("payload_size", RECORD_PAYLOAD_SIZES)
@pytest.mark.asyncio
async def test_query_unique(
    api_client, admin_user, sample_format: repoclient.Format, payload_size: int
):
    data = []
    for i in range(0, payload_size):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == payload_size

    query = repoclient.Query(query=[], format_id=[sample_format.id])
    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_records = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, per_page=1_000, pagination_strategy=strategy
        ):
            numeric_value = item.data["NumericColumn"]
            string_value = item.data["StringColumn"]
            assert (
                int(string_value) == numeric_value
            ), "numeric value not equal to string value"
            assert (
                numeric_value not in seen_records
            ), "received duplicate/invalid record"
            seen_records.add(numeric_value)
        # make sure we have exactly MAX_RECORDS records
        assert len(seen_records) == payload_size
        assert min(seen_records) == 0
        assert max(seen_records) == (payload_size - 1)

    # Test dataframe
    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    dataframe["NumericColumn"] = dataframe["NumericColumn"].astype("float")

    unique_values = len(list(dataframe["NumericColumn"].unique()))
    assert unique_values == (payload_size), f"Expected {payload_size} unique values"
    assert dataframe["NumericColumn"].min() == 0, "Min check failed"
    assert dataframe["NumericColumn"].max() == payload_size - 1, "Max check failed"
    assert len(dataframe.index) == payload_size, "Dataframe has wrong dimensions"
    assert {"NumericColumn", "StringColumn"}.issubset(
        set(dataframe.columns)
    ), "CSV contains unexpected columns"


@pytest.mark.parametrize("payload_size", RECORD_PAYLOAD_SIZES)
@pytest.mark.asyncio
async def test_query_negated(
    api_client, admin_user, sample_format: repoclient.Format, payload_size: int
):
    data = []
    string_data = []
    for i in range(0, payload_size):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        string_data.append(entry["StringColumn"])
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == payload_size
    # test with the same condition as above but this time invert the whole thing
    # this should return all records that are less than 50
    group = repoclient.QueryGroup(
        args=[
            # query all numbers that are greater or equal than 50
            repoclient.Column(column="NumericColumn") >= payload_size // 2,
            # also query all strings that are in the string_data (basically the same condition as above)
            repoclient.Column(column="StringColumn").is_in(string_data),
        ]
    ).negate()
    query = repoclient.Query(query=[group], format_id=[sample_format.id])
    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_records = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            assert item.data["NumericColumn"] < payload_size // 2
            seen_records.add(item.data["NumericColumn"])
        # make sure we have exactly 50 records
        assert (
            len(seen_records) == payload_size // 2
        ), f"wrong number of records returned (got {len(seen_records)}"

    # Test dataframe
    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    dataframe["NumericColumn"] = dataframe["NumericColumn"].astype("float")

    unique_values = len(list(dataframe["NumericColumn"].unique()))
    assert unique_values == (
        payload_size // 2
    ), f"Expected {payload_size} unique values"
    assert dataframe["NumericColumn"].min() == 0, "Min check failed"
    assert (
        dataframe["NumericColumn"].max() == (payload_size // 2) - 1
    ), "Max check failed"
    assert len(dataframe.index) == payload_size // 2, "Dataframe has wrong dimensions"
    assert {"NumericColumn", "StringColumn"}.issubset(
        set(dataframe.columns)
    ), "CSV contains unexpected columns"


@pytest.mark.parametrize("payload_size", RECORD_PAYLOAD_SIZES)
@pytest.mark.asyncio
async def test_query_empty(
    api_client, admin_user, sample_format: repoclient.Format, payload_size: int
):
    data = []
    string_data = []
    for i in range(0, payload_size):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        string_data.append(entry["StringColumn"])
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == payload_size
    # test with the same condition as above but this time invert the whole thing
    # this should return all records that are less than 50
    half = payload_size // 2
    group = repoclient.QueryGroup(
        args=[
            # query all numbers that are greater or equal than 50
            repoclient.Column(column="NumericColumn")
            >= half,
        ]
    ).negate()
    # this group is exactly the opposite of the above group: it'll
    # return all records whose StringColumn is between "51" and "100"
    second_group = repoclient.QueryGroup(
        args=[
            repoclient.Column(column="StringColumn").is_in(string_data[half:]),
            repoclient.Column(column="NumericColumn") >= half,
        ]
    )
    query = repoclient.Query(query=[group, second_group], format_id=[sample_format.id])
    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        count = 0
        async for _ in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            count += 1
        # make sure there are no records
        assert count == 0, f"count should be 0 but got {count}"

    # Test dataframe (we just need to make sure it's empty)
    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    assert len(dataframe.index) == 0, "Dataframe wasn't empty"
    assert {"NumericColumn", "StringColumn"}.issubset(
        set(dataframe.columns)
    ), "CSV contains unexpected columns"


@pytest.mark.asyncio
async def test_query_like_case_insensitive(
    api_client, admin_user, sample_format: repoclient.Format
):
    data = [
        {"NumericColumn": 0, "StringColumn": "Star Wars: The last jedi"},
        {"NumericColumn": 0, "StringColumn": "Star Wars: The force awakens"},
        {"NumericColumn": 0, "StringColumn": "Star Wars: The empire strikes back"},
        # this movie doesn't have "the" in the title, so it should be skipped
        {"NumericColumn": 0, "StringColumn": "Star Wars: Attack of the clones"},
    ]
    expected_titles = set([item["StringColumn"] for item in data])

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == len(data)
    second_group = repoclient.QueryGroup(
        args=[
            repoclient.Column(column="StringColumn").is_like_case_insensitive(
                "star wars: the %"
            ),
        ]
    )
    query = repoclient.Query(query=[second_group], format_id=[sample_format.id])
    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_titles = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            seen_titles.add(item.data["StringColumn"])
        # make sure there are two records
        assert seen_titles.issubset(expected_titles), "invalid titles"
        assert len(seen_titles) == 3, "invalid number of titles"
        assert "Star Wars: attack of the clones" not in seen_titles, "invalid titles"

    # Check dataframe
    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    unique_values = set(list(dataframe["StringColumn"]))
    # Make sure         {"NumericColumn": 0, "StringColumn": "Star Wars: Attack of the clones"},
    # is not contained in the retrieved values (since it does not start with "the".
    for value in unique_values:
        assert "attack" not in value.lower()
    assert len(unique_values) == 3
    assert unique_values.issubset(expected_titles)


@pytest.mark.asyncio
async def test_query_regex_case_insensitive(
    api_client, admin_user, sample_format: repoclient.Format
):
    data = [
        {"NumericColumn": 0, "StringColumn": "Star Wars: THE LAST JEDI"},
        {"NumericColumn": 0, "StringColumn": "Star Wars: THE FORCE AWAKENS"},
    ]
    expected_titles = set([item["StringColumn"] for item in data])

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == len(data)
    second_group = repoclient.QueryGroup(
        args=[
            repoclient.Column(column="StringColumn").matches_regex_case_insensitive(
                "star wars: the.*"
            ),
        ]
    )
    query = repoclient.Query(query=[second_group], format_id=[sample_format.id])
    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_titles = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            seen_titles.add(item.data["StringColumn"])
        # make sure there are two records
        assert seen_titles.issubset(expected_titles), "invalid titles"
        assert len(seen_titles) == 2, "invalid number of titles"

    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    unique_values = set(list(dataframe["StringColumn"]))
    assert len(unique_values) == 2
    assert unique_values.issubset(expected_titles)


@pytest.mark.asyncio
async def test_query_regex_case_sensitive(
    api_client, admin_user, sample_format: repoclient.Format
):
    data = [
        {"NumericColumn": 0, "StringColumn": "Star Wars: THE LAST JEDI"},
        {"NumericColumn": 0, "StringColumn": "Star Wars: THE FORCE AWAKENS"},
    ]

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == len(data)
    second_group = repoclient.QueryGroup(
        args=[
            # this will only match "Star Wars: THE FORCE AWAKENS" (this condition
            # is negated).
            repoclient.Column(column="StringColumn").matches_regex(".*LAST JEDI$"),
        ]
    ).negate()
    query = repoclient.Query(query=[second_group], format_id=[sample_format.id])
    # use all available pagination strategies
    for strategy in list(repoclient.PaginationStrategy):
        seen_titles = set()
        async for item in sample_format.get_data(
            api_client, admin_user, query, pagination_strategy=strategy
        ):
            seen_titles.add(item.data["StringColumn"])
        # make sure there is one record
        assert len(seen_titles) == 1, "invalid number of titles"
        assert list(seen_titles)[0] == "Star Wars: THE FORCE AWAKENS", "invalid titles"

    dataframe = await load_streaming_query_into_df(
        api_client, admin_user, sample_format, query
    )
    unique_values = set(list(dataframe["StringColumn"]))
    assert len(unique_values) == 1
