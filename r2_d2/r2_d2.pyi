from typing import Optional, Protocol


class R2Usage(Protocol):
    end: str
    payload_size: int
    metadata_size: int
    object_count: int
    upload_count: int
    infrequent_access_payload_size: int
    infrequent_access_metadata_size: int
    infrequent_access_object_count: int
    infrequent_access_upload_count: int

    def __str__(self) -> str: ...

    def __repr__(self) -> str: ...


async def usage() -> R2Usage: ...


async def main_rs() -> int: ...
