import click
import asyncio
from functools import wraps
import pandas
from httpx import AsyncClient
from sys import stdout
import repoclient as rc

loop = asyncio.get_event_loop()


def sync_to_async(f):
    # sync to async wrapper for click functions
    # see https://github.com/pallets/click/issues/85
    @wraps(f)
    def wrapper(*args, **kwargs):
        return loop.run_until_complete(f(*args, **kwargs))

    return wrapper


@click.group()
@click.argument("upstream")
@click.argument("auth")
@click.pass_context
@sync_to_async
async def cli(ctx: click.Context, upstream: str, auth: str):
    """Repoclient's CLI utility.

    This tool allows you to invoke some of the most
    commonly used API endpoints. Note that this tool
    is limited to some degree: it does not support all
    endpoints nor all the parameters. You should always
    use the RESTful interface over this tool.

    You can authenticate using either an existing API token,
    or using a username and password.

    Example:

    1. Auth with exiting token: python -m repoclient token:ABCD... <ACTION>

    2. Auth with username & password: python -m repoclient basic:username:password <ACTION>
    """
    ctx.obj = {}
    if upstream.endswith("/"):
        upstream = upstream[:-1]

    client = AsyncClient(base_url=upstream, verify=False, timeout=60)
    ctx.obj["client"] = client
    mode_credentials = auth.split(":", maxsplit=1)
    if len(mode_credentials) != 2:
        raise Exception(
            "Invalid auth format: %s. Expected: '<mode>:<credentials>'" % auth
        )
    mode, credentials = mode_credentials
    # Auth mode: user + password

    if mode == "basic":
        user_password = credentials.split(":", maxsplit=1)
        if len(user_password) != 2:
            raise Exception(
                "Invalid auth format: %s. Expected: '<username>:<password>'"
                % credentials
            )
        username, password = credentials.split(":")
        try:
            user = await rc.User(username=username, password=password).login(client)
        except Exception as e:
            click.secho("Cannot authenticate: %s" % e, fg="red", bold=True)
            exit(-1)
        ctx.obj["user"] = user
    elif mode == "token":
        user = rc.User.from_api_key(credentials.strip())
        ctx.obj["user"] = user
    else:
        click.secho(
            "Invalid auth mode: %s. Possible choices: 'basic', 'token" % mode,
            fg="red",
            bold=True,
        )
        exit(-1)


@click.command(help="Get short-lived token using username & password")
@click.pass_context
@sync_to_async
async def get_token(ctx: click.Context):
    user: rc.User = ctx.obj["user"]
    click.secho("%s" % user.token)


@click.command(help="List all API keys")
@click.pass_context
@sync_to_async
async def list_api_key(ctx: click.Context):
    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    # async with client:
    async for item in user.get_all_keys(client):
        click.secho("%s" % item)


@click.command()
@click.argument("key_id")
@click.option("--user-id")
@click.pass_context
@sync_to_async
async def delete_api_key(ctx: click.Context, key_id: str, user_id: str = None):
    """Deletes an API key by ID. If you're an admin, you must
    additionally pass a User ID to delete a key for that specific
    user ID.

    Example:

    python -m repoclient http://localhost:8000 token:abcd... delete-api-key 5bdabd10-4406-4f2d-9356-127a804c2639
    """

    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    if user.is_superuser is True and user_id is None:
        click.secho(
            (
                "Cannot delete API key for superuser. "
                "Please specify an user using --user-id"
            ),
            fg="red",
            bold=True,
        )

        exit(-1)
    if user_id is None:
        user_id = user.id

    try:
        await rc.UserApiKey.delete_by_id(client, user, user_id, key_id)
        click.secho("Key %s deleted successfully for user %s" % (key_id, user_id))
    except Exception as e:
        click.secho("Cannot delete key %s: %s" % (key_id, e), fg="red", bold=True)
        exit(-1)


@click.command(help="Create API Key for regular user")
@click.pass_context
@sync_to_async
async def create_api_key(ctx: click.Context):
    user: rc.User = ctx.obj["user"]
    if user.is_superuser:
        click.secho("Cannot create API key for superuser", fg="red", bold=True)
        exit(-1)
    client: AsyncClient = ctx.obj["client"]
    api_key = await user.create_api_key(client)
    click.secho("%s" % api_key)
    click.secho("%s" % api_key.token, fg="green", bold=True)


@click.command()
@click.argument("key_id")
@click.option("--user-id", help="(Optional) Rotate key for User ID")
@click.option("--understand-risk", is_flag=True, default=False, help="Accept warning")
@click.pass_context
@sync_to_async
async def rotate_api_key(
    ctx: click.Context, key_id: str, user_id: str = None, understand_risk: bool = False
):
    """Rotates an existing API key.

    Note: This call is potentially dangerous as it will invalidate
    the previous API key, potentially disrupting any existing apps
    using it. Pass --understand-risk to override this warning.

    Example:

    python -m repoclient http://localhost:8000 token:abcd... rotate-api-key 5bdabd10-4406-4f2d-9356-127a804c2639 --understand-risk
    """
    user: rc.User = ctx.obj["user"]
    if user.is_superuser is False and understand_risk is False:
        click.secho(
            (
                "Rotating a key will invalidate and de-authenticate any"
                " processes using it. If you're sure you want to rotate"
                " this key, please pass --understand-risk."
            ),
            fg="red",
            bold=True,
        )
        exit(-1)
    if user.is_superuser is True and user_id is None:
        click.secho(
            (
                "Cannot delete API key for superuser. "
                "Please specify an user using --user-id"
            ),
            fg="red",
            bold=True,
        )
        exit(-1)

    if user_id is None:
        user_id = user.id
    client: AsyncClient = ctx.obj["client"]
    api_key = await rc.UserApiKey.rotate(client, user, user_id, key_id)
    click.secho("%s" % api_key)
    click.secho("%s" % api_key.token, fg="green", bold=True)


@click.command()
@click.pass_context
@sync_to_async
async def list_upload_session(ctx: click.Context):
    """List all existing upload sessions.

    Note: You'll only be able to see upload sessions for
    formats you have access to. If you don't have an entitlement
    for any given format, you won't be able to view its upload sessions.
    """
    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    try:
        async for item in rc.UploadSession.get_all(client, user):
            click.secho("%s" % item)
    except Exception as e:
        click.secho("Cannot list sessions: %s" % e, fg="red", bold=True)
        exit(-1)


@click.command()
@click.argument("session_id")
@click.pass_context
@sync_to_async
async def delete_upload_session(ctx: click.Context, session_id: str):
    """Deletes an existing upload session.

    Note: You must have either the `limitedDelete` or `delete` entitlements
    for the format you want to delete data from. Otherwise,
    this request will fail.

    Example:

    python -m repoclient http://localhost:8000 token:abcd... delete-upload-session 5bdabd10-4406-4f2d-9356-127a804c2639
    """
    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    try:
        await rc.UploadSession.delete_by_id(client, user, session_id)
        click.secho("Session %s deleted successfully" % session_id)
    except Exception as _:
        click.secho(
            "Cannot delete session. Maybe you don't have access to that format?",
            fg="red",
            bold=True,
        )
        exit(-1)


@click.command()
@click.pass_context
@sync_to_async
async def list_format(ctx: click.Context):
    """List all existing formats.

    Note: You'll only be able to see formats you have access to.
    """
    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    async for item in rc.Format.get_all(client, user):
        click.secho("%s" % item)


@click.command()
@click.argument("format_id")
@click.option(
    "--override-check",
    is_flag=True,
    default=False,
    help="Write to interactive terminal",
)
@click.pass_context
@sync_to_async
async def dump_format_data(ctx: click.Context, format_id: str, override_check: bool):
    """Dump a format's contents.

    Note: You'll need a `read` entitlement for the format you're trying
    to read from.

    Additionally, this command will dump the entire contents for
    that specific format. If you want to filter out special entries,
    please use the library or the RESTful API interface.
    """
    if stdout.isatty() is True and override_check is False:
        click.secho(
            (
                "Refusing to write data inside an interactive session. "
                " Please pipe the output to a file, i.e. `command > out.csv` or "
                " use --override-check to continue anyway."
            ),
            fg="red",
            bold=True,
        )
        exit(-1)

    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    try:
        fmt = await rc.Format.get(client, format_id, user)
        query = rc.Query(query=[], format_id=[format_id])
        await fmt.get_data_csv_stream(client, user, query, stdout.buffer)
    except Exception as err:
        click.secho("Cannot find format %s: %s" % (format_id, err), fg="red", bold=True)
        exit(-1)


@click.command()
@click.argument("path")
@click.argument("format_id")
@click.option(
    "--cast-types",
    is_flag=True,
    default=True,
    help="Cast data types (enabled by default)",
)
@click.option(
    "--fill-na",
    is_flag=True,
    default=False,
    help="Whether to fill nulls or not (disabled by default)",
)
@click.pass_context
@sync_to_async
async def upload_format_data(
    ctx: click.Context, path: str, format_id: str, cast_types: bool, fill_na: bool
):
    """Upload a format's contents.

    Note: You'll need a `write` entitlement for the format you're trying
    to write to.
    """
    user: rc.User = ctx.obj["user"]
    client: AsyncClient = ctx.obj["client"]
    try:
        fmt = await rc.Format.get(client, format_id, user)
    except Exception as err:
        click.secho("Cannot find format %s: %s" % (format_id, err), fg="red", bold=True)
        exit(-1)

    try:
        df = pandas.read_csv(path)
        session = await fmt.upload_from_dataframe(client, user, df, cast_types, fill_na)
        click.secho("%s" % session, fg="green", bold=True)
    except Exception as e:
        click.secho("Cannot upload data: %s" % e, fg="red", bold=True)
        exit(-1)


cli.add_command(get_token)
cli.add_command(list_api_key)
cli.add_command(delete_api_key)
cli.add_command(create_api_key)
cli.add_command(rotate_api_key)
cli.add_command(list_upload_session)
cli.add_command(delete_upload_session)
cli.add_command(list_format)
cli.add_command(dump_format_data)
cli.add_command(upload_format_data)

if __name__ == "__main__":
    cli()
