from datetime import datetime
from typing import Tuple, Any, Callable

import repoclient
import pytest
import operator

from .util import get_random_string, api_client, admin_user
from repoclient import ColumnSchema, FormatUploadSession, FormatUploadSessionFilter, P


@pytest.mark.asyncio
async def test_create_format(api_client, admin_user):
    columns = []
    column_names = []
    for _ in range(0, 5):
        numeric_col_name = get_random_string(10)
        numeric_column = ColumnSchema.numeric(numeric_col_name)
        string_col_name = get_random_string(10)
        string_column = ColumnSchema.string(string_col_name)
        columns.append(numeric_column)
        columns.append(string_column)
        column_names.append(numeric_col_name)
        column_names.append(string_col_name)
    column_names = set(column_names)

    name = get_random_string(10)
    fmt = await repoclient.Format(
        name=name, description="some nice description", schema=columns
    ).create(api_client, admin_user)
    assert fmt.id is not None, "format id is None"
    # retrieve format back
    fmt = await repoclient.Format.get(api_client, fmt.id, admin_user)
    # collect column types
    col_names = set([col.name for col in fmt.schema_ref])
    # make sure the server returns everything we sent
    assert fmt.id is not None, "format id is None"
    assert len(fmt.schema_ref) == 10, "wrong number of columns"
    assert fmt.name == name, "wrong format name"
    assert fmt.description == "some nice description", "wrong format description"
    assert col_names == column_names, "wrong format schema"
    # query records for this format (there should be 0)
    query = repoclient.Query(query=[], format_id=[fmt.id])
    record_count = await fmt.get_count(api_client, admin_user, query)
    assert record_count == 0, "format has records"
    # Clean up
    await fmt.delete(api_client, admin_user)


@pytest.mark.parametrize(
    "compare",
    # (compare against, whether to expect an exception or not)
    [
        (123, False),
        (0, False),
        ("0", True),
        (0.0, True),
        (datetime.now(), True),
        (None, True),
    ],
)
@pytest.mark.parametrize(
    "field",
    [
        FormatUploadSessionFilter.ID,
        FormatUploadSessionFilter.RECORD_COUNT,
        FormatUploadSessionFilter.FORMAT_ID,
    ],
)
@pytest.mark.parametrize(
    "comp_op",
    [
        (operator.ge, "Gte"),
        (operator.le, "Lte"),
        (operator.gt, "Gt"),
        (operator.lt, "Lt"),
        (operator.eq, "Eq"),
    ],
)
def test_upload_parameters_integer(
    field: FormatUploadSessionFilter,
    comp_op: Tuple[Callable[[Any, Any], P], str],
    compare: Tuple[Any, bool],
):
    """Test upload parameter query builder."""
    op, expected_str_oper = comp_op
    compare, expect_exception = compare

    # The following is basically the equivalent of doing something like
    # P("field") > 123, P("field") < 123, etc.
    conditions = [op(P(field), compare)]
    if expect_exception:
        with pytest.raises(AssertionError) as _exc:
            # This means we're trying to compare something else, like
            # a string, datetime, etc.
            _ = FormatUploadSession(args=conditions)
    else:
        upload_session_query = FormatUploadSession(args=conditions)
        result = upload_session_query.prepared_args
        query_string = upload_session_query.as_url_params()
        assert len(result) == 1
        assert query_string == f"{field.value}{expected_str_oper}={compare}"
        assert result[0] == (f"{field.value}{expected_str_oper}", compare)
