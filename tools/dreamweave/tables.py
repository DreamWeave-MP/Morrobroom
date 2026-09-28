"""Strict reading of TOML tables: every key is either understood or reported."""

import datetime
import difflib
import re
import uuid
from urllib.parse import urlsplit

from .problems import Problems
from .versions import DECIMAL, NUMERIC, Constraint, Version, VersionError

TOKEN_PATTERN = re.compile(r"^[a-z0-9][a-z0-9-]*$")
EXTENSION_NAMESPACE_PATTERN = re.compile(r"^[a-z0-9][a-z0-9-]*(\.[a-z0-9][a-z0-9-]*)+$")
MISSING = object()


class Table:
    """A TOML table being read. Call `finish()` to report keys nobody asked for."""

    def __init__(self, data: object, where: str, problems: Problems):
        self.where = where
        self.problems = problems
        self.valid = isinstance(data, dict)
        self.data = data if isinstance(data, dict) else {}
        self.seen: set[str] = set()
        if not self.valid:
            problems.error(where, f"must be a table, got {type(data).__name__}")

    def child_where(self, key: str) -> str:
        return f"{self.where}.{key}"

    def has(self, key: str) -> bool:
        return key in self.data

    def raw(self, key: str, default: object = MISSING) -> object:
        self.seen.add(key)
        if key in self.data:
            return self.data[key]
        if default is MISSING:
            self.problems.error(self.child_where(key), "is required")
            return None
        return default

    def string(self, key: str, default: object = MISSING, pattern: re.Pattern | None = None, describe: str = "") -> str | None:
        value = self.raw(key, default)
        if value is None or (default is not MISSING and value is default):
            return value
        if not isinstance(value, str):
            self.problems.error(self.child_where(key), f"must be a string, got {type(value).__name__}")
            return None
        if not value.strip():
            self.problems.error(self.child_where(key), "must not be empty")
            return None
        if pattern and not pattern.match(value):
            self.problems.error(self.child_where(key), f"{value!r} is not {describe or 'valid'}")
            return None
        return value

    def boolean(self, key: str, default: bool) -> bool:
        value = self.raw(key, default)
        if not isinstance(value, bool):
            self.problems.error(self.child_where(key), f"must be true or false, got {value!r}")
            return default
        return value

    def integer(self, key: str, default: object = MISSING) -> int | None:
        value = self.raw(key, default)
        if value is None or value is default:
            return value
        if isinstance(value, bool) or not isinstance(value, int):
            self.problems.error(self.child_where(key), f"must be an integer, got {value!r}")
            return None
        return value

    def choice(self, key: str, choices: tuple[str, ...], default: str) -> str:
        value = self.raw(key, default)
        if value not in choices:
            self.problems.error(
                self.child_where(key),
                f"{value!r} is not one of {', '.join(choices)}{suggestion(value, choices)}",
            )
            return default
        return value

    def string_list(self, key: str, default: list | None = None, pattern: re.Pattern | None = None, describe: str = "") -> list[str]:
        value = self.raw(key, [] if default is None else default)
        if not isinstance(value, list):
            self.problems.error(self.child_where(key), f"must be a list of strings, got {type(value).__name__}")
            return []

        strings = []
        for index, item in enumerate(value):
            where = f"{self.child_where(key)}[{index}]"
            if not isinstance(item, str) or not item.strip():
                self.problems.error(where, f"must be a non-empty string, got {item!r}")
            elif pattern and not pattern.match(item):
                self.problems.error(where, f"{item!r} is not {describe or 'valid'}")
            else:
                strings.append(item)

        duplicates = sorted({item for item in strings if strings.count(item) > 1})
        if duplicates:
            self.problems.error(self.child_where(key), f"lists {', '.join(map(repr, duplicates))} more than once")
        return strings

    def table(self, key: str) -> "Table":
        return Table(self.raw(key, {}), self.child_where(key), self.problems)

    def table_list(self, key: str) -> list["Table"]:
        value = self.raw(key, [])
        if not isinstance(value, list):
            self.problems.error(self.child_where(key), f"must be an array of tables ([[{key}]]), got {type(value).__name__}")
            return []
        return [Table(item, f"{self.child_where(key)}[{index}]", self.problems) for index, item in enumerate(value)]

    def date(self, key: str) -> str | None:
        value = self.raw(key)
        if value is None:
            return None
        if isinstance(value, datetime.datetime) or not isinstance(value, datetime.date):
            self.problems.error(self.child_where(key), f"must be a TOML date like 2026-09-28, got {value!r}")
            return None
        return value.isoformat()

    def version(self, key: str, scheme: str = NUMERIC) -> Version | None:
        value = self.raw(key)
        if value is None:
            return None
        try:
            return Version.parse(value, scheme)
        except VersionError as error:
            self.problems.error(self.child_where(key), str(error))
            return None

    def constraint(self, key: str, default: object = MISSING, scheme: str = NUMERIC) -> Constraint | None:
        value = self.raw(key, default)
        if value is None or value is default:
            return value
        try:
            return Constraint.parse(value, scheme)
        except VersionError as error:
            self.problems.error(self.child_where(key), str(error))
            return None

    def uuid(self, key: str, default: object = MISSING) -> str | None:
        if default is MISSING and key not in self.data:
            self.seen.add(key)
            self.problems.error(self.child_where(key), f'is required. Here is a fresh one to paste: {key} = "{uuid.uuid4()}"')
            return None
        value = self.string(key, default)
        if value is None or value is default:
            return value
        return check_uuid(value, self.child_where(key), self.problems)

    def url(self, key: str, default: object = MISSING) -> str | None:
        value = self.string(key, default)
        if value is None or value is default:
            return value
        return check_url(value, self.child_where(key), self.problems)

    def finish(self) -> None:
        for key in sorted(set(self.data) - self.seen):
            known = sorted(self.seen)
            self.problems.error(self.child_where(key), f"is not a recognized key{suggestion(key, known)}")


def suggestion(value: object, choices) -> str:
    if not isinstance(value, str):
        return ""
    close = difflib.get_close_matches(value, list(choices), n=1)
    return f" (did you mean {close[0]!r}?)" if close else ""


def check_uuid(value: str, where: str, problems: Problems) -> str | None:
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        problems.error(where, f"{value!r} is not a UUID. Use a fresh random one, like {uuid.uuid4()}")
        return None
    if str(parsed) != value:
        problems.error(where, f"write the UUID in canonical lowercase form: {parsed}")
        return None
    if parsed.int == 0 or parsed.int == (1 << 128) - 1:
        problems.error(where, "the nil and max UUIDs cannot identify a project")
        return None
    return value


def check_url(value: str, where: str, problems: Problems) -> str | None:
    parts = urlsplit(value)
    if parts.scheme not in ("https", "http") or not parts.netloc:
        problems.error(where, f"{value!r} must be an absolute http(s) URL")
        return None
    if any(character.isspace() for character in value):
        problems.error(where, f"{value!r} contains whitespace; percent-encode it")
        return None
    return value
