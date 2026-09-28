"""Git access. Packaging reads committed blobs, never the working tree.

Reading blobs makes an archive a function of a commit: line-ending conversion, editor droppings
and untracked build output cannot leak into it, and CI can rebuild a tag without checking it out.
"""

import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import PurePosixPath


class GitError(RuntimeError):
    pass


@dataclass(frozen=True)
class TreeEntry:
    path: str
    mode: str
    blob: str


def run_git(*arguments: str, input_bytes: bytes | None = None) -> bytes:
    process = subprocess.run(["git", *arguments], input=input_bytes, capture_output=True)
    if process.returncode != 0:
        message = process.stderr.decode(errors="replace").strip()
        raise GitError(f"git {' '.join(arguments)} failed: {message}")
    return process.stdout


def require_repository() -> None:
    try:
        run_git("rev-parse", "--show-toplevel")
    except (GitError, FileNotFoundError) as error:
        raise GitError(
            "this is not a git checkout (or git is not installed). DreamWeave packages committed "
            "files, so it needs the repository's history."
        ) from error


def resolve_revision(revision: str) -> str:
    return run_git("rev-parse", "--verify", "--quiet", f"{revision}^{{commit}}").decode().strip()


def tag_revision(tag: str) -> str | None:
    process = subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"],
        capture_output=True,
    )
    if process.returncode != 0:
        return None
    return process.stdout.decode().strip()


def commit_time(revision: str) -> str:
    stamp = run_git("show", "-s", "--format=%ct", revision).decode().strip()
    moment = datetime.fromtimestamp(int(stamp), tz=timezone.utc)
    return moment.strftime("%Y-%m-%dT%H:%M:%SZ")


def count_commits(revision: str, since: str | None, path: str, excluding: tuple[str, ...] = ()) -> int:
    span = f"{since}..{revision}" if since else revision
    pathspecs = [path, *(f":(exclude){excluded}" for excluded in excluding)]
    return int(run_git("rev-list", "--count", span, "--", *pathspecs).decode().strip())


def tree_entries(revision: str, directory: str) -> list[TreeEntry]:
    output = run_git("ls-tree", "-r", "-z", "--full-tree", revision, "--", directory)
    entries = []
    for record in output.split(b"\0"):
        if not record:
            continue
        metadata, path = record.split(b"\t", 1)
        mode, _kind, blob = metadata.decode().split(" ")
        entries.append(TreeEntry(path=path.decode("utf-8"), mode=mode, blob=blob))
    return entries


def working_tree_files(directory: str) -> list[str]:
    """Tracked and untracked-but-not-ignored files: what the next commit would contain."""
    output = run_git("ls-files", "--cached", "--others", "--exclude-standard", "-z", "--", directory)
    return sorted({path.decode("utf-8") for path in output.split(b"\0") if path})


def read_blobs(blobs: list[str]) -> dict[str, bytes]:
    """Read blobs one request at a time so a large payload is held in memory once, not twice."""
    contents: dict[str, bytes] = {}
    if not blobs:
        return contents

    process = subprocess.Popen(["git", "cat-file", "--batch"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        for blob in sorted(set(blobs)):
            process.stdin.write(f"{blob}\n".encode())
            process.stdin.flush()
            header = process.stdout.readline().decode().strip().split(" ")
            if len(header) != 3 or header[1] != "blob" or header[0] != blob:
                raise GitError(f"expected blob {blob}, git returned {' '.join(header)}")
            contents[blob] = process.stdout.read(int(header[2]))
            process.stdout.read(1)
    finally:
        process.stdin.close()
        process.wait()
    return contents


def dirty_paths(paths: list[str]) -> list[str]:
    output = run_git("status", "--porcelain=v1", "-z", "--untracked-files=all", "--", *paths)
    return [record[3:].decode("utf-8") for record in output.split(b"\0") if len(record) > 3]


def relative_to(path: str, directory: str) -> str:
    return str(PurePosixPath(path).relative_to(PurePosixPath(directory)))
