import asyncio

from .r2_d2 import *  # src/lib.rs


async def usage_async() -> R2Usage:
    return await usage()


def usage_sync() -> R2Usage:
    return asyncio.run(usage_async())


async def async_main_py():
    """
    Async entrypoint ('main_rs' can't be used with asyncio.run directly)
    """
    exit(await main_rs())  # returns exit code


def main():
    """
    Sync entrypoint.
    Using asyncio allows using async rust code (via tokio).
    """
    print(
        repr(usage_sync())
    )
    asyncio.run(async_main_py())


if __name__ == "__main__":
    main()
