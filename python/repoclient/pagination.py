import asyncio
import math
from pprint import pformat
from concurrent.futures import ProcessPoolExecutor
from typing import Iterator, Tuple, TypeVar
from pydantic import TypeAdapter
from httpx import AsyncClient, Response
import concurrent
import logging
from os import environ

from repoclient.exception import RepositoryError
from repoclient.models.user import User
from enum import Enum, auto

T = TypeVar("T")

logger = logging.getLogger("repoclient")

POOL_EXECUTOR_SIZE = environ.get("POOL_EXECUTOR_SIZE", None)
MAX_RETRIES = environ.get("MAX_RETRIES", 3)
# Cast POOL_EXECUTOR_SIZE to int if set
if POOL_EXECUTOR_SIZE is not None:
    POOL_EXECUTOR_SIZE = int(POOL_EXECUTOR_SIZE)

if MAX_RETRIES is not None:
    MAX_RETRIES = int(MAX_RETRIES)

QUEUE_SENTINEL = None
PER_PAGE_DEFAULT = 10_000
MAX_CONCURRENT = 8
POOL_EXECUTOR = concurrent.futures.ProcessPoolExecutor(POOL_EXECUTOR_SIZE)


async def _make_paginated_request(
    client: AsyncClient,
    upstream: str,
    pydantic_model: T,
    page: int,
    per_page: int,
    method: str = "GET",
    headers=None,
    json=None,
    count: bool = False,
    retries: int = MAX_RETRIES,
) -> Tuple[Response, list[T]]:
    """Make a repository/paginated call.

    Parameters:
    upstream: Upstream URL
    pydantic_model: Deserialize as this model
    page: Page to fetch
    per_page: Pull this many items
    method: HTTP Method
    headers: HTTP Headers
    json: JSON Body
    count: Whether to pull the item count or not for this request
    """
    count = "true" if count else "false"
    url = f"{upstream}page={page}&perPage={per_page}&count={count}"
    logger.debug(
        "url: %s page: %s, per_page: %s, method: %s", url, page, per_page, method
    )
    while True:
        try:
            response = await client.request(method, url, headers=headers, json=json)
            RepositoryError.verify_raise_conditionally(response)
            break
        except Exception as e:
            if retries == 0:
                logger.error("reached max retries (%s)", MAX_RETRIES, exc_info=e)
                raise e from e
            retries -= 1
            logger.warning(
                "retrying failed request (attempting %s more times, max: %s)",
                retries,
                MAX_RETRIES,
                exc_info=e,
            )

    try:
        deserialized = TypeAdapter(pydantic_model).validate_json(response.text)
    except Exception as e:
        logger.error("Deserialize error: content: %s", exc_info=e)
        raise e from e
    return response, deserialized


class PaginationStrategy(Enum):
    """
    The strategy to use when paginating.


    ``FAST``:
        The default strategy. This will query the repository in a sequential
        fashion. It's the optimal option for queries that don't need to pull
        a lot of data.
        This strategy will basically call the parallel code with a semaphore
        of size 1.

    ``PARALLEL``:
        This strategy will issue multiple requests concurrently in the most
        efficient way. You can control how many requests are issued in parallel
        using the `max_concurrency` kwarg when you use this strategy.
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
        method: str = "GET",
        json=None,
    ) -> int:
        """
        Returns the total number of items that match the query.
        """
        if not upstream.endswith("&"):
            upstream += "?"
        # we only want the item count headers.
        url = f"{upstream}page=0&perPage=1"
        logger.debug(f"fetching url: {url}")
        response = await client.request(method, url, headers=user.bearer, json=json)
        RepositoryError.verify_raise_conditionally(response)
        return int(response.headers.get("repository-item-count", 0))

    @staticmethod
    async def get_all(
        *,
        upstream: str,
        klass: T,
        client: AsyncClient,
        user: User,
        per_page: int = PER_PAGE_DEFAULT,
        pagination_strategy: PaginationStrategy = PaginationStrategy.FAST,
        method: str = "GET",
        json=None,
        **kwargs,
    ) -> Iterator[T]:
        logger.debug("using pagination strategy: %s", pagination_strategy)
        if json is not None and logger.level <= logging.DEBUG:
            logger.debug("sending query: \n%s", pformat(json, indent=2))

        if pagination_strategy is PaginationStrategy.FAST:
            strategy_fn = PaginatedResponse._get_all_fast
        elif pagination_strategy is PaginationStrategy.PARALLEL:
            strategy_fn = PaginatedResponse._get_all_parallel
        else:
            raise RuntimeError(f"Unimplemented strategy: {pagination_strategy}")
        async for items in strategy_fn(
            upstream=upstream,
            klass=klass,
            client=client,
            user=user,
            per_page=per_page,
            json=json,
            method=method,
            **kwargs,
        ):
            yield items

    @staticmethod
    async def _get_all_fast(
        *,
        upstream: str,
        klass: T,
        client: AsyncClient,
        user: User,
        method: str,
        per_page: int = PER_PAGE_DEFAULT,
        json=None,
    ) -> Iterator[list[T]]:
        async for item in PaginatedResponse._get_all_parallel(
            upstream=upstream,
            klass=klass,
            client=client,
            user=user,
            method=method,
            per_page=per_page,
            json=json,
            max_concurrency=1,
        ):
            yield item

    @staticmethod
    def _check_and_warn_max_concurrency(max_concurrency: int):
        # Check if the passed max_concurrency is a power of two.
        #
        # This has a direct impact on performance, because the database
        # is single-threaded, and most CPUs have a number of CPUs that is
        # a power of two.
        power_of_two = math.log(max_concurrency, 2)
        is_power_of_two = power_of_two == int(power_of_two)

        if is_power_of_two:
            logger.info("max_concurrency: semaphore permits: %s ", max_concurrency)
        else:
            logger.warning(
                "max_concurrency should be a power of 2 to better "
                "leverage the multicore capabilities of the repository."
                " Currently set to %s.",
                max_concurrency,
            )

    @staticmethod
    async def _get_all_parallel(
        *,
        upstream: str,
        klass: T,
        client: AsyncClient,
        user: User,
        method: str,
        per_page: int = PER_PAGE_DEFAULT,
        json=None,
        max_concurrency: int = MAX_CONCURRENT,
    ) -> Iterator[list[T]]:
        assert max_concurrency > 0, "max_concurrency must be greater than 0"
        # if this URL doesn't already have query parameters,
        # add the query parameter delimiter ("?") to it.
        if not upstream.endswith("&"):
            upstream += "?"

        PaginatedResponse._check_and_warn_max_concurrency(max_concurrency)

        # Get the first page (index 0)
        url = f"{upstream}page=0&perPage={per_page}&count=true"
        logger.debug(f"fetching first request URL: {url}")
        (response, deserialized) = await _make_paginated_request(
            client=client,
            upstream=upstream,
            pydantic_model=klass,
            page=0,
            per_page=per_page,
            method=method,
            headers=user.bearer,
            json=json,
            count=True,
        )
        RepositoryError.verify_raise_conditionally(response)
        yield deserialized

        page_count: int = int(response.headers.get("repository-page-count"))
        item_count: int = int(response.headers.get("repository-item-count"))
        logger.debug(
            "page count: %s, item count: %s, max_concurrency: %s, pool size (POOL_EXECUTOR_SIZE): %s",
            page_count,
            item_count,
            max_concurrency,
            POOL_EXECUTOR_SIZE,
        )
        # yield items from first request
        # check if there are no more pages
        if page_count <= 1:
            logger.debug("there are no more items, returning")
            return

        coroutines = []

        # Make the request and put the received data in the queue.
        async def make_request_queued(sem: asyncio.Semaphore, page: int, **kwargs):
            async with sem:
                logger.debug("acquired semaphore N#: %s", page)
                try:
                    (response, deserialized) = await _make_paginated_request(
                        page=page, **kwargs
                    )
                    await queue.put(deserialized)
                    logger.debug("%s: fetched %s items", url, len(deserialized))
                except Exception as exc:
                    logger.error(
                        "Couldn't fetch data (kwargs: %s)",
                        kwargs,
                        exc_info=exc,
                    )
                    await queue.put(QUEUE_SENTINEL)

        queue = asyncio.Queue()
        semaphore = asyncio.Semaphore(max_concurrency)

        coroutines = [
            make_request_queued(
                sem=semaphore,
                page=page,
                client=client,
                upstream=upstream,
                pydantic_model=klass,
                per_page=per_page,
                method=method,
                headers=user.bearer,
                json=json,
                count=False,
            )
            for page in range(1, page_count)
        ]

        async def schedule_coroutines():
            await asyncio.gather(*coroutines)

        concurrent_tasks_fut = asyncio.create_task(schedule_coroutines())
        logger.debug("%s coroutines have been fired off!", len(coroutines))
        received_pages_count = 1

        while received_pages_count < page_count:
            received_pages_count += 1
            percent = (received_pages_count / page_count) * 100

            try:
                items = await queue.get()
                queue.task_done()
            except asyncio.CancelledError:
                # Stop polling the remaining futures if this
                # future gets cancelled (aka ctrl-c was pressed)
                concurrent_tasks_fut.cancel()
                logger.info("cancelled %s tasks", len(coroutines))
                break

            if items is QUEUE_SENTINEL:
                concurrent_tasks_fut.cancel()
                raise RuntimeError(
                    "A request worker has crashed. Please check the logs for "
                    "more details."
                )

            logger.debug(
                "received so far: %s of %s pages (%.2f%%)",
                received_pages_count,
                page_count,
                percent,
            )
            yield items
