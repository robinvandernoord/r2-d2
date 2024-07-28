import asyncio

from .r2_d2 import *  # src/lib.rs


async def usage_async() -> R2Usage:
    return await usage()

async def error_async() -> R2Usage:
    return await error()

async def main_py_async():
    """
    Async entrypoint ('main_rs' can't be used with asyncio.run directly)
    """
    try:
        print('pre')
        exit_code = await main_rs()
        print('success')
    except RuntimeError as e:
        print(":3")
        raise ValueError("Something went wrong in Rust land") from e
        exit_code = 1
    except BaseException as e:
        print(f"Unexpected error {type(e)}", e)
    finally:
        print('post')

    exit(exit_code)


# ---

def usage_sync() -> R2Usage:
    return asyncio.run(usage_async())

def error_sync() -> R2Usage:
    return asyncio.run(error_async())

def main_py_sync():
    asyncio.run(main_py_async())

def main():
    """
    Sync entrypoint.
    Using asyncio allows using async rust code (via tokio).
    """
    # print(
    #     repr(usage_sync())
    # )
    # error_sync()

    main_py_sync()

if __name__ == "__main__":
    main()
