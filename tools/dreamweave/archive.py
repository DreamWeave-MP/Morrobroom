"""Byte-reproducible zip archives.

Every entry is stored (not compressed), sorted by path, dated 1980-01-01, marked as created on
Unix with 0644 or 0755 permissions, and named in UTF-8. Nothing depends on the machine, the
clock, the Python version or the zlib build, so the same inputs always produce the same bytes
and the same SHA-256.

Compression is deliberately absent. Deflate output differs between zlib and zlib-ng (Fedora
ships the latter), so a compressed archive built on a laptop cannot be checked against one
built in CI. On a real audio-heavy mod, deflate saved 5%.
"""

import hashlib
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path

LOCAL_HEADER_SIGNATURE = 0x04034B50
CENTRAL_HEADER_SIGNATURE = 0x02014B50
END_OF_CENTRAL_DIRECTORY_SIGNATURE = 0x06054B50
ZIP64_END_SIGNATURE = 0x06064B50
ZIP64_LOCATOR_SIGNATURE = 0x07064B50
ZIP64_EXTRA_ID = 0x0001

UTF8_FLAG = 0x0800
DOS_DATE_1980_01_01 = (0 << 9) | (1 << 5) | 1
DOS_TIME_MIDNIGHT = 0
UNIX_SYSTEM = 3
VERSION_DEFAULT = 20
VERSION_ZIP64 = 45
LIMIT_32 = 0xFFFFFFFF
LIMIT_16 = 0xFFFF


@dataclass(frozen=True)
class ArchiveEntry:
    path: str
    executable: bool
    content: bytes


@dataclass(frozen=True)
class ArchiveResult:
    path: Path
    size: int
    sha256: str
    entries: int


class HashingWriter:
    def __init__(self, stream):
        self.stream = stream
        self.digest = hashlib.sha256()
        self.position = 0

    def write(self, data: bytes) -> None:
        self.stream.write(data)
        self.digest.update(data)
        self.position += len(data)


def write_archive(destination: Path, entries: list[ArchiveEntry]) -> ArchiveResult:
    ordered = sorted(entries, key=lambda entry: entry.path.encode("utf-8"))
    names = [entry.path for entry in ordered]
    duplicates = sorted({name for name in names if names.count(name) > 1})
    if duplicates:
        raise ValueError(f"archive would contain duplicate paths: {', '.join(duplicates)}")

    destination.parent.mkdir(parents=True, exist_ok=True)
    central_records = []
    with destination.open("wb") as stream:
        writer = HashingWriter(stream)

        for entry in ordered:
            offset = writer.position
            name = entry.path.encode("utf-8")
            size = len(entry.content)
            checksum = zlib.crc32(entry.content) & LIMIT_32
            needs_zip64 = size >= LIMIT_32

            local_extra = struct.pack("<HHQQ", ZIP64_EXTRA_ID, 16, size, size) if needs_zip64 else b""
            recorded_size = LIMIT_32 if needs_zip64 else size
            version = VERSION_ZIP64 if needs_zip64 else VERSION_DEFAULT
            writer.write(struct.pack(
                "<IHHHHHIIIHH",
                LOCAL_HEADER_SIGNATURE, version, UTF8_FLAG, 0, DOS_TIME_MIDNIGHT, DOS_DATE_1980_01_01,
                checksum, recorded_size, recorded_size, len(name), len(local_extra),
            ))
            writer.write(name)
            writer.write(local_extra)
            writer.write(entry.content)
            central_records.append((entry, name, size, checksum, offset))

        central_offset = writer.position
        for entry, name, size, checksum, offset in central_records:
            zip64_fields = []
            if size >= LIMIT_32:
                zip64_fields += [size, size]
            if offset >= LIMIT_32:
                zip64_fields.append(offset)
            central_extra = b""
            if zip64_fields:
                central_extra = struct.pack(f"<HH{len(zip64_fields)}Q", ZIP64_EXTRA_ID, 8 * len(zip64_fields), *zip64_fields)
            version = VERSION_ZIP64 if zip64_fields else VERSION_DEFAULT
            permissions = 0o100755 if entry.executable else 0o100644
            writer.write(struct.pack(
                "<IHHHHHHIIIHHHHHII",
                CENTRAL_HEADER_SIGNATURE, (UNIX_SYSTEM << 8) | version, version, UTF8_FLAG, 0,
                DOS_TIME_MIDNIGHT, DOS_DATE_1980_01_01, checksum,
                LIMIT_32 if size >= LIMIT_32 else size, LIMIT_32 if size >= LIMIT_32 else size,
                len(name), len(central_extra), 0, 0, 0, permissions << 16,
                LIMIT_32 if offset >= LIMIT_32 else offset,
            ))
            writer.write(name)
            writer.write(central_extra)

        central_size = writer.position - central_offset
        count = len(central_records)
        if count >= LIMIT_16 or central_size >= LIMIT_32 or central_offset >= LIMIT_32:
            zip64_end_offset = writer.position
            writer.write(struct.pack(
                "<IQHHIIQQQQ", ZIP64_END_SIGNATURE, 44, (UNIX_SYSTEM << 8) | VERSION_ZIP64, VERSION_ZIP64,
                0, 0, count, count, central_size, central_offset,
            ))
            writer.write(struct.pack("<IIQI", ZIP64_LOCATOR_SIGNATURE, 0, zip64_end_offset, 1))
            writer.write(struct.pack(
                "<IHHHHIIH", END_OF_CENTRAL_DIRECTORY_SIGNATURE, 0, 0,
                min(count, LIMIT_16), min(count, LIMIT_16), min(central_size, LIMIT_32), min(central_offset, LIMIT_32), 0,
            ))
        else:
            writer.write(struct.pack(
                "<IHHHHIIH", END_OF_CENTRAL_DIRECTORY_SIGNATURE, 0, 0, count, count, central_size, central_offset, 0,
            ))

    return ArchiveResult(path=destination, size=writer.position, sha256=writer.digest.hexdigest(), entries=count)
