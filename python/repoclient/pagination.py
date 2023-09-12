import asyncio
import functools
import multiprocessing
import os
import orjson
from pprint import pformat
from concurrent.futures import ProcessPoolExecutor
from typing import Iterator, Callable, Optional
from pydantic import parse_obj_as
from httpx import AsyncClient, Response
import logging

from repoclient.models.user import User
from enum import Enum, auto

logger = logging.getLogger("repoclient")

QUEUE_SENTINEL = None
PER_PAGE_DEFAULT = 10_000
MAX_CONCURRENT = 10


class PaginationStrategy(Enum):
    """
    The strategy to use when paginating.


    ``FAST``:
        This strategy will pull data until the server returns an empty
        response. This might be considerably faster for queries where
        a large number of rows were matched.
        This will disable the server-side pagination and item counting, at
        the expense of having to issue an extra, empty request (this tells us
        we've queried all the available data).

    ``PARALLEL``:
        This strategy will issue multiple requests concurrently in the most
        efficient way. First, a request using the default strategy is
        issued to get the total number of pages. After that, the ``FAST``
        strategy is used to pull all remaining data concurrently.
        This strategy is faster than all the other strategies, but might consume
        more memory.
    """

    PARALLEL = auto()
    FAST = auto()


class PaginatedResponse:
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
        per_page: int = PER_PAGE_DEFAULT,
        pagination_strategy: PaginationStrategy = PaginationStrategy.FAST,
        json=None,
        **kwargs,
    ) -> Iterator[object]:
        assert exc_handler is not None, "Exception handler is None"
        logger.debug("using pagination strategy: %s", pagination_strategy)
        if json is not None and logger.level <= logging.DEBUG:
            logger.debug("sending query: \n%s", pformat(json, indent=2))

        if pagination_strategy == PaginationStrategy.FAST:
            strategy_fn = PaginatedResponse._get_all_fast
        elif pagination_strategy == PaginationStrategy.PARALLEL:
            strategy_fn = PaginatedResponse._get_all_parallel_nowait
        async for items in strategy_fn(
            upstream=upstream,
            klass=klass,
            client=client,
            user=user,
            exc_handler=exc_handler,
            per_page=per_page,
            json=json,
            **kwargs,
        ):
            yield items

    @staticmethod
    async def _get_all_fast(
        *,
        upstream: str,
        klass: object,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        per_page: int = PER_PAGE_DEFAULT,
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
            yield ret
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
    async def _get_all_parallel_nowait(
        *,
        upstream: str,
        klass: object,
        client: AsyncClient,
        user: User,
        exc_handler: Callable[[Response], None],
        per_page: int = PER_PAGE_DEFAULT,
        json=None,
        max_concurrency: int = MAX_CONCURRENT,
    ) -> Iterator[object]:
        assert max_concurrency > 0, "max_concurrency must be greater than 0"
        # if this URL doesn't already have query parameters,
        # add the query parameter delimiter ("?") to it.
        if not upstream.endswith("&"):
            upstream += "?"

        # Get the first page (index 0)
        url = f"{upstream}page=0&perPage={per_page}&count=true"
        logger.debug(f"fetching first request URL: {url}")
        response = await client.request("GET", url, headers=user.bearer, json=json)
        response_items = PaginatedResponse._response_to_object(
            response, klass, exc_handler
        )
        page_count: int = int(response.headers.get("repository-page-count"))
        item_count: int = int(response.headers.get("repository-item-count"))
        logger.debug(
            "page count: %s, item count: %s, max_concurrency: %s",
            page_count,
            item_count,
            max_concurrency,
        )
        # yield items from first request
        yield response_items
        # check if there are no more pages
        if page_count <= 1:
            logger.debug("there are no more items, returning")
            return

        coroutines = []

        # Make the request and put the received data in the queue.
        async def make_request_queued(
            url: str,
            headers: dict,
            json: str,
            queue: asyncio.Queue,
            semaphore: asyncio.Semaphore,
        ):
            async with semaphore:
                try:
                    response = await client.request(
                        "GET", url, headers=user.bearer, json=json
                    )
                    retval = parse_obj_as(list[klass], orjson.loads(response.text))
                    await queue.put(retval)
                    logger.debug("%s: fetched %s items", url, len(retval))
                except Exception as exc:
                    logger.error(
                        "Couldn't fetch data (URL: %s, headers: %s, payload: %s)",
                        url,
                        headers,
                        json,
                        exc_info=exc,
                    )
                    await queue.put(QUEUE_SENTINEL)

        queue = asyncio.Queue()
        semaphore = asyncio.Semaphore(max_concurrency)

        for page in range(1, page_count):
            # note: use "FAST" strategy from now on, since we now know the
            # total number of pages.
            url = f"{upstream}page={page}&perPage={per_page}&count=false"
            coroutines.append(
                make_request_queued(url, user.bearer, json, queue, semaphore)
            )

        # Schedule execution
        async def run_concurrent():
            await asyncio.gather(*coroutines)

        concurrent_tasks_fut = asyncio.create_task(run_concurrent())

        logger.debug("%s coroutines have been fired off!", len(coroutines))
        received_pages_count = 1
        while received_pages_count < page_count:
            received_pages_count += 1
            logger.debug("received so far: %s/%s", received_pages_count, page_count)
            items = await queue.get()
            queue.task_done()
            if items is QUEUE_SENTINEL:
                logger.warning(
                    "at least one request failed: cancelling remaining tasks"
                )
                concurrent_tasks_fut.cancel()
                raise RuntimeError(
                    "An unexpected error happened. Check the logs for more details."
                )
            yield items
