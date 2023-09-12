import repoclient
import pytest
import logging

from .util import get_random_string, api_client, admin_user, sample_format, normal_user
from repoclient import ColumnSchema


logging.getLogger("repoclient").setLevel(logging.DEBUG)


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
        upload = await sample_format.upload_data(api_client, admin_user, data)
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
        upload = await sample_format.upload_data(api_client, normal_user, data)
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


@pytest.mark.asyncio
async def test_query(api_client, admin_user, sample_format: repoclient.Format):
    data = []
    string_data = []
    for i in range(0, 100):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        string_data.append(entry["StringColumn"])
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == 100
    group = repoclient.QueryGroup(
        args=[
            # query all numbers that are greater or equal than 50
            repoclient.Column(column="NumericColumn") >= 50,
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
            assert item.data["NumericColumn"] >= 50
            seen_records.add(item.data["NumericColumn"])
        # make sure we have exactly 50 records
        assert len(seen_records) == 50


@pytest.mark.asyncio
async def test_query_unique(api_client, admin_user, sample_format: repoclient.Format):
    data = []
    string_data = []
    MAX_RECORDS = 10_000
    for i in range(0, MAX_RECORDS):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == MAX_RECORDS

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
        assert len(seen_records) == MAX_RECORDS
        assert min(seen_records) == 0
        assert max(seen_records) == (MAX_RECORDS - 1)


@pytest.mark.asyncio
async def test_query_negated(api_client, admin_user, sample_format: repoclient.Format):
    data = []
    string_data = []
    for i in range(0, 100):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        string_data.append(entry["StringColumn"])
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == 100
    # test with the same condition as above but this time invert the whole thing
    # this should return all records that are less than 50
    group = repoclient.QueryGroup(
        args=[
            # query all numbers that are greater or equal than 50
            repoclient.Column(column="NumericColumn") >= 50,
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
            assert item.data["NumericColumn"] < 50
            seen_records.add(item.data["NumericColumn"])
        # make sure we have exactly 50 records
        assert (
            len(seen_records) == 50
        ), f"wrong number of records returned (got {len(seen_records)}"


@pytest.mark.asyncio
async def test_query_empty(api_client, admin_user, sample_format: repoclient.Format):
    data = []
    string_data = []
    for i in range(0, 100):
        entry = {"NumericColumn": i, "StringColumn": f"{i}"}
        string_data.append(entry["StringColumn"])
        data.append(entry)

    upload = await sample_format.upload_data(api_client, admin_user, data)
    assert upload.outcome == "Success"
    assert upload.record_count == 100
    # test with the same condition as above but this time invert the whole thing
    # this should return all records that are less than 50
    group = repoclient.QueryGroup(
        args=[
            # query all numbers that are greater or equal than 50
            repoclient.Column(column="NumericColumn")
            >= 50,
        ]
    ).negate()
    # this group is exactly the opposite of the above group: it'll
    # return all records whose StringColumn is between "51" and "100"
    second_group = repoclient.QueryGroup(
        args=[
            repoclient.Column(column="StringColumn").is_in(string_data[50:]),
            repoclient.Column(column="NumericColumn") >= 50,
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
