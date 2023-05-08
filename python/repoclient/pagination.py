import asyncio
import functools
import multiprocessing
import os
from pprint import pformat
from multiprocessing import Pool, JoinableQueue
from typing import Iterator, Callable, Optional
from pydantic import parse_obj_as
from httpx import AsyncClient, Response
import logging

from repoclient.models.user import User
from enum import Enum, auto

logger = logging.getLogger("repoclient")

POOL: Optional[Pool] = None


class PaginationStrategy(Enum):
    """
    The strategy to use when paginating.


    ``DEFAULT``:
        The default strategy. This strategy will query the server
        in such way that it returns the total number of pages and items
        for each request. The client will issue the exact number of requests
        needed to get all data.

        This strategy has the downside of being slow for queries where an unusually
        large number of rows were matched in the server. The server has to issue
        two SQL queries: a ``COUNT`` and a ``SELECT``, in order to retrieve the data
        and the total number of items that matched this query. Doing a ``COUNT``
        implies searching for all the matching data, even if the server returns only
        the first 1000 items.

        This isn't be a problem for rather basic, "normal" queries, i.e. queries
        that return 50,000 or fewer items.

    ``FAST``:
        This strategy will pull data until the server returns an empty
        response. This might be considerably faster for queries where
        a large number of rows were matched.
        This will disable the server-side pagination and item counting, at
        the expense of having to issue an extra, empty request (this tells us
        we've queried all the available data). This strategy isn't considerably
        faster for small queries to justify an extra request.

    ``PARALLEL``:
        This strategy will issue multiple requests concurrently in the most
        efficient way. First, a request using the default strategy is
        issued to get the total number of pages. After that, the ``FAST``
        strategy is used to pull all remaining data concurrently.
        This strategy is faster than all other strategies with the downside
        of having to use a lot of memory, depending on the amount of items
        returned by the server.
    """

    DEFAULT = auto()
    PARALLEL = auto()
    FAST = auto()


class PaginatedResponse:
    @staticmethod
    def init_pool():
        global POOL
        POOL = Pool()

    @staticmethod
    async def log_request_id(response: Response):
        logger.debug("request id: %s", response.headers.get("request-id", None))

    @staticmethod
    async def get_count(
        *,
        upstream: str,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        json=None,
    ) -> int:
        """
        Returns the total number of items that match the query.
        """
        assert exc_handler is not None, "Exception handler is None"
        if not upstream.endswith("&"):
            upstream += "?"
        # we only want the item count headers.
        url = f"{upstream}page=0&perPage=1"
        logger.debug(f"fetching url: {url}")
        response = await client.request("GET", url, headers=user.bearer, json=json)
        if response.status_code != 200:
            exc_handler(response)
        item_count: int = int(response.headers.get("repository-item-count"))
        logger.debug("item count: %s", item_count)
        return item_count

    @staticmethod
    async def get_all(
        *,
        upstream: str,
        klass: object,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        per_page: int = 1000,
        pagination_strategy: PaginationStrategy = PaginationStrategy.DEFAULT,
        json=None,
    ) -> Iterator[object]:
        assert exc_handler is not None, "Exception handler is None"
        logger.debug("using pagination strategy: %s", pagination_strategy)
        strategy_fn = PaginatedResponse.get_all_default
        if json is not None and logger.level <= logging.DEBUG:
            logger.debug("sending query: \n%s", pformat(json, indent=2))

        if pagination_strategy == PaginationStrategy.FAST:
            strategy_fn = PaginatedResponse.get_all_fast
        elif pagination_strategy == PaginationStrategy.PARALLEL:
            strategy_fn = PaginatedResponse.get_all_parallel
        async for item in strategy_fn(
            upstream=upstream,
            klass=klass,
            client=client,
            user=user,
            exc_handler=exc_handler,
            per_page=per_page,
            json=json,
        ):
            yield item

    @staticmethod
    async def get_all_default(
        *,
        upstream: str,
        klass: object,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        per_page: int = 1000,
        json=None,
    ) -> Iterator[object]:
        current_page = 0
        page_count = 1
        # if this URL doesn't already have query parameters,
        # add the query parameter delimiter ("?") to it.
        if not upstream.endswith("&"):
            upstream += "?"
        while True:
            # note: page count starts at 1
            if current_page > page_count - 1:
                logger.debug("there are no more items, returning")
                break
            url = f"{upstream}page={current_page}&perPage={per_page}"
            logger.debug(f"fetching url: {url}")
            response = await client.request("GET", url, headers=user.bearer, json=json)
            if response.status_code != 200:
                exc_handler(response)
            page_count: int = int(response.headers.get("repository-page-count"))
            logger.debug("server returned a new page count: %s", page_count)
            ret = parse_obj_as(list[klass], response.json())
            logger.debug("yielding %s items", len(ret))
            for item in ret:
                yield item
            current_page += 1

    @staticmethod
    async def get_all_fast(
        *,
        upstream: str,
        klass: object,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        per_page: int = 1000,
        json=None,
    ) -> Iterator[object]:
        current_page = 0
        if not upstream.endswith("&"):
            upstream += "?"
        while True:
            url = f"{upstream}page={current_page}&perPage={per_page}&count=false"
            logger.debug(f"fetching url: {url}")
            response = await client.request("GET", url, headers=user.bearer, json=json)
            if response.status_code != 200:
                exc_handler(response)
            ret = parse_obj_as(list[klass], response.json())
            if len(ret) == 0:
                logger.debug("received empty response, returning")
                break
            logger.debug("yielding %s items", len(ret))
            for item in ret:
                yield item
            current_page += 1

    @staticmethod
    def _response_to_object(
        response: Response,
        klass: object,
        exc_handler: Callable[[Response], None],
        check_status: bool = True,
    ) -> list[object]:
        if check_status and response.status_code != 200:
            exc_handler(response)
        ret = parse_obj_as(list[klass], response.json())
        logger.debug("deserialized %s items", len(ret))
        return ret

    @staticmethod
    async def get_all_parallel(
        *,
        upstream: str,
        klass: object,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        per_page: int = 1000,
        json=None,
    ) -> Iterator[object]:
        logger.warning(
            """\
Using parallel pagination strategy is not recommended \
as it might consume too much memory. Unless you know what \
you're doing, please use either the default or fast \
pagination strategies."""
        )
        current_page = 0
        # if this URL doesn't already have query parameters,
        # add the query parameter delimiter ("?") to it.
        if not upstream.endswith("&"):
            upstream += "?"

        url = f"{upstream}page={current_page}&perPage={per_page}"
        logger.debug(f"fetching first request URL: {url}")
        # first request contains total page count
        response = await client.request("GET", url, headers=user.bearer, json=json)
        response_items = PaginatedResponse._response_to_object(
            response, klass, exc_handler
        )
        page_count: int = int(response.headers.get("repository-page-count"))
        item_count: int = int(response.headers.get("repository-item-count"))
        logger.debug("page count: %s, item count: %s", page_count, item_count)
        # yield items from first request
        for item in response_items:
            yield item
        # check if there are no more pages
        if page_count <= 1:
            logger.debug("there are no more items, returning")
            return

        # there are more pages, prepare coroutines for all subsequent pages
        coroutines = []
        for page in range(1, page_count):
            # note: use "FAST" strategy from now on, since we now know the
            # total number of pages.
            url = f"{upstream}page={page}&perPage={per_page}&count=false"
            coroutines.append(
                client.request("GET", url, headers=user.bearer, json=json)
            )

        logger.debug("%s coroutines have been fired off!", len(coroutines))
        responses = await asyncio.gather(*coroutines)
        logger.debug("verifying status code of %s responses", len(responses))
        # do not deserialize responses if one of them failed
        for response in responses:
            if response.status_code != 200:
                exc_handler(response)
        # proxy function that only receives a Response object
        pool_fn = functools.partial(
            PaginatedResponse._response_to_object,
            klass=klass,
            exc_handler=exc_handler,
            check_status=False,
        )
        # deserialize responses in parallel
        global POOL

        logger.debug("parallel deserialize")
        results = POOL.starmap_async(pool_fn, ((r,) for r in responses))
        for items in results.get():
            for item in items:
                yield item
