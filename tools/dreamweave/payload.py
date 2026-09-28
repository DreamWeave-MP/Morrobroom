"""A project's payload: every committed file under its directory, checked against mod.toml.

Documentation sources (index.md, docs/) ship on purpose. mod.lock does not: it records the
archive's own hash, so including it would be circular.
"""

import os
import re
import stat
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from . import gitrepo
from .model import MOD_LOCK, Project
from .problems import Problems

REGULAR_FILE = "100644"
EXECUTABLE_FILE = "100755"
SYMLINK = "120000"
SUBMODULE = "160000"
WINDOWS_RESERVED_NAMES = {"con", "prn", "aux", "nul", *(f"com{number}" for number in range(1, 10)), *(f"lpt{number}" for number in range(1, 10))}
WINDOWS_RESERVED_CHARACTERS = re.compile(r'[<>:"|?*\\\x00-\x1f]')
GENERATED_ARCHIVE_ROOTS = ("Documentation", "fomod", "dreamweave.release.json")


@dataclass(frozen=True)
class PayloadFile:
    path: str
    executable: bool
    blob: str | None
    disk_path: Path | None


def working_tree_entries(root: Path, directory: str) -> list[gitrepo.TreeEntry]:
    entries = []
    for path in gitrepo.working_tree_files(directory):
        disk_path = root / path
        if not os.path.lexists(disk_path):
            continue
        mode = os.lstat(disk_path).st_mode
        if stat.S_ISLNK(mode):
            git_mode = SYMLINK
        elif stat.S_ISDIR(mode):
            git_mode = SUBMODULE
        else:
            git_mode = EXECUTABLE_FILE if mode & stat.S_IXUSR else REGULAR_FILE
        entries.append(gitrepo.TreeEntry(path=path, mode=git_mode, blob=""))
    return entries


def collect_payload(project: Project, revision: str | None, nested_project_directories: list[str], problems: Problems, root: Path | None = None) -> list[PayloadFile]:
    """Committed files at `revision`, or with revision None, what the working tree would commit."""
    where = f"{project.directory} at {revision[:12]}" if revision else project.directory
    tree = gitrepo.tree_entries(revision, project.directory) if revision else working_tree_entries(root or Path.cwd(), project.directory)
    files = []
    for entry in tree:
        relative = gitrepo.relative_to(entry.path, project.directory)
        if relative == MOD_LOCK:
            continue
        if any(entry.path.startswith(f"{nested}/") for nested in nested_project_directories):
            continue
        if entry.mode == SYMLINK:
            problems.error(where, f"{relative!r} is a symlink; archives carry files, and extracting links is how archives escape their directory")
            continue
        if entry.mode == SUBMODULE:
            problems.error(where, f"{relative!r} is a git submodule; its files are not in this repository's history")
            continue
        if entry.mode not in (REGULAR_FILE, EXECUTABLE_FILE):
            problems.error(where, f"{relative!r} has unsupported git mode {entry.mode}")
            continue
        check_portable_path(relative, where, problems)
        files.append(PayloadFile(
            path=relative,
            executable=entry.mode == EXECUTABLE_FILE,
            blob=entry.blob or None,
            disk_path=None if revision else (root or Path.cwd()) / entry.path,
        ))

    by_folded_path: dict[str, list[str]] = {}
    for file in files:
        by_folded_path.setdefault(file.path.casefold(), []).append(file.path)
    for paths in by_folded_path.values():
        if len(paths) > 1:
            problems.error(where, f"{' and '.join(map(repr, sorted(paths)))} differ only by case; Windows and OpenMW's VFS treat them as one file")

    for file in files:
        root = PurePosixPath(file.path).parts[0]
        if root in GENERATED_ARCHIVE_ROOTS:
            problems.error(where, f"{file.path!r} collides with {root!r}, which DreamWeave generates inside the archive")

    check_components_against_payload(project, files, where, problems)
    return files


def check_portable_path(path: str, where: str, problems: Problems) -> None:
    for segment in path.split("/"):
        if WINDOWS_RESERVED_CHARACTERS.search(segment):
            problems.error(where, f"{path!r} contains a character Windows cannot store in a file name")
            return
        if segment.endswith((" ", ".")):
            problems.error(where, f"{path!r} has a segment ending in a space or dot, which Windows silently strips")
            return
        if segment.split(".")[0].casefold() in WINDOWS_RESERVED_NAMES:
            problems.error(where, f"{path!r} uses a name Windows reserves for devices")
            return


def check_components_against_payload(project: Project, files: list[PayloadFile], where: str, problems: Problems) -> None:
    paths = [file.path for file in files]
    folded = {path.casefold(): path for path in paths}

    def files_under(directory: str) -> list[str]:
        if directory == ".":
            return paths
        prefix = f"{directory}/"
        return [path for path in paths if path.startswith(prefix)]

    for component in project.components:
        component_where = f"{where} component {component.id!r}"
        if not files_under(component.path):
            problems.error(component_where, f"path {component.path!r} has no files (ignored files do not count)")
            continue

        data_roots = [join(component.path, directory) for directory in component.openmw.data_directories]
        for data_root in data_roots:
            if not files_under(data_root):
                problems.error(component_where, f"data directory {data_root!r} has no files (ignored files do not count)")

        for label, names in (
            ("content_files", component.openmw.content_files),
            ("groundcover_files", component.openmw.groundcover_files),
            ("fallback_archives", component.openmw.fallback_archives),
        ):
            for name in names:
                candidates = [join(data_root, name) for data_root in data_roots]
                if any(candidate in paths for candidate in candidates):
                    continue
                wrong_case = [folded[candidate.casefold()] for candidate in candidates if candidate.casefold() in folded]
                if wrong_case:
                    problems.error(component_where, f"openmw.{label} names {name!r} but the file is {wrong_case[0]!r}; case matters")
                else:
                    problems.error(component_where, f"openmw.{label} names {name!r}, which is not in any of its data directories ({', '.join(data_roots)})")

        if component.openmw.config and join(component.path, "openmw.cfg") not in paths:
            problems.error(component_where, "openmw.config is true but the component has no openmw.cfg")


def join(directory: str, name: str) -> str:
    if directory == ".":
        return name
    if name == ".":
        return directory
    return f"{directory}/{name}"


def undeclared_content_files(project: Project, files: list[PayloadFile]) -> list[str]:
    declared = set()
    for component in project.components:
        for directory in component.openmw.data_directories:
            root = join(component.path, directory)
            for name in component.openmw.content_files + component.openmw.groundcover_files:
                declared.add(join(root, name))
    suffixes = (".esm", ".esp", ".omwgame", ".omwaddon", ".omwscripts")
    return sorted(file.path for file in files if file.path.lower().endswith(suffixes) and file.path not in declared)
