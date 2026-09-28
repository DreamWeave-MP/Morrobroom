"""DreamWeave versions, their two ordering schemes, and version constraints.

A version is dot-separated release numbers with an optional SemVer-style pre-release and build
suffix: `1`, `0.51`, `1.2.0`, `2.0.0-beta.3`, `1.4.0+el9`. Pre-release and build rules are
SemVer 2.0.0 §9-11; build metadata never changes precedence.

A project picks how its release numbers compare, and its manifest says which:

- numeric (default): every number compares as an integer and missing numbers count as zero.
  1.2 = 1.2.0 < 1.2.9 < 1.2.10, and 0.9 < 0.82.
- decimal: the first number is an integer; every later number compares like the digits after a
  decimal point, and trailing zeros do not count. 0.5 = 0.50 < 0.54 < 0.6 < 0.82 < 0.9 < 0.963.

A constraint is `*` or comma-separated comparators that must all hold: `>=0.49, <0.51`.
Operators are `=`, `!=`, `>`, `>=`, `<` and `<=`. There is no `^`, `~` or `||`. A constraint on
another project is evaluated with that project's scheme, which its manifest states.
"""

import re
from dataclasses import dataclass
from functools import total_ordering

NUMERIC = "numeric"
DECIMAL = "decimal"
SCHEMES = (NUMERIC, DECIMAL)
MAXIMUM_NUMBERS = 6

INTEGER = r"(?:0|[1-9][0-9]*)"
DIGITS = r"[0-9]+"
IDENTIFIER = r"[0-9A-Za-z-]+"
SUFFIX = rf"(?:-(?P<prerelease>{IDENTIFIER}(?:\.{IDENTIFIER})*))?(?:\+(?P<build>{IDENTIFIER}(?:\.{IDENTIFIER})*))?$"
PATTERNS = {
    NUMERIC: re.compile(rf"^(?P<release>{INTEGER}(?:\.{INTEGER}){{0,{MAXIMUM_NUMBERS - 1}}}){SUFFIX}"),
    DECIMAL: re.compile(rf"^(?P<release>{INTEGER}(?:\.{DIGITS}){{0,{MAXIMUM_NUMBERS - 1}}}){SUFFIX}"),
}
COMPARATOR_PATTERN = re.compile(r"^(?P<operator>>=|<=|!=|=|>|<)\s*(?P<version>\S+)$")


class VersionError(ValueError):
    pass


def check_scheme(scheme: str) -> str:
    if scheme not in SCHEMES:
        raise VersionError(f"unknown versioning scheme {scheme!r}; use numeric or decimal")
    return scheme


@total_ordering
@dataclass(frozen=True)
class Version:
    text: str
    scheme: str
    release_text: tuple[str, ...]
    prerelease: tuple[int | str, ...]

    @classmethod
    def parse(cls, text: str, scheme: str = NUMERIC) -> "Version":
        check_scheme(scheme)
        if not isinstance(text, str):
            raise VersionError(f"version must be a string, got {type(text).__name__} {text!r}")

        match = PATTERNS[scheme].match(text)
        if not match:
            leading_zero = scheme == NUMERIC and PATTERNS[DECIMAL].match(text)
            hint = " (a number after the first has a leading zero; that only means something with versioning = \"decimal\")" if leading_zero else ""
            raise VersionError(
                f"{text!r} is not a DreamWeave version: use numbers separated by dots, "
                f"optionally followed by -prerelease or +build (for example 1.2.0 or 2.0.0-beta.1){hint}"
            )

        prerelease: tuple[int | str, ...] = ()
        if match.group("prerelease"):
            identifiers = []
            for identifier in match.group("prerelease").split("."):
                if identifier.isdigit():
                    if len(identifier) > 1 and identifier.startswith("0"):
                        raise VersionError(f"{text!r}: numeric pre-release identifier {identifier!r} has a leading zero")
                    identifiers.append(int(identifier))
                else:
                    identifiers.append(identifier)
            prerelease = tuple(identifiers)

        return cls(text=text, scheme=scheme, release_text=tuple(match.group("release").split(".")), prerelease=prerelease)

    def release_key(self) -> tuple:
        if self.scheme == NUMERIC:
            numbers = tuple(int(number) for number in self.release_text)
            return numbers + (0,) * (MAXIMUM_NUMBERS - len(numbers))
        fractions = tuple(digits.rstrip("0") for digits in self.release_text[1:])
        return (int(self.release_text[0]), fractions + ("",) * (MAXIMUM_NUMBERS - 1 - len(fractions)))

    def precedence_key(self) -> tuple:
        if not self.prerelease:
            return (self.release_key(), 1, ())
        identifiers = tuple(
            (0, identifier, "") if isinstance(identifier, int) else (1, 0, identifier)
            for identifier in self.prerelease
        )
        return (self.release_key(), 0, identifiers)

    def _comparable(self, other: object) -> "Version":
        if not isinstance(other, Version):
            return NotImplemented
        if other.scheme != self.scheme:
            raise TypeError(f"cannot compare a {self.scheme} version with a {other.scheme} version")
        return other

    def __eq__(self, other: object) -> bool:
        other = self._comparable(other)
        if other is NotImplemented:
            return NotImplemented
        return self.precedence_key() == other.precedence_key()

    def __lt__(self, other: "Version") -> bool:
        other = self._comparable(other)
        if other is NotImplemented:
            return NotImplemented
        return self.precedence_key() < other.precedence_key()

    def __hash__(self) -> int:
        return hash((self.scheme, self.precedence_key()))

    def __str__(self) -> str:
        return self.text

    @property
    def is_prerelease(self) -> bool:
        return bool(self.prerelease)

    def next_development(self, build_number: int) -> "Version":
        """The development build that sorts after this version and before any successor.

        numeric: 1.2.0 -> 1.2.1-dev.N. decimal: 0.963 -> 0.9631-dev.N, 1 -> 1.001-dev.N.
        After a pre-release: 2.0.0-beta.1 -> 2.0.0-beta.1.dev.N in either scheme.
        """
        release = ".".join(self.release_text)
        if self.prerelease:
            prerelease = ".".join(str(identifier) for identifier in self.prerelease)
            return Version.parse(f"{release}-{prerelease}.dev.{build_number}", self.scheme)
        if self.scheme == NUMERIC:
            bumped = self.release_text[:-1] + (str(int(self.release_text[-1]) + 1),)
            return Version.parse(".".join(bumped) + f"-dev.{build_number}", self.scheme)
        if len(self.release_text) == 1:
            return Version.parse(f"{release}.001-dev.{build_number}", self.scheme)
        return Version.parse(f"{release}1-dev.{build_number}", self.scheme)


@dataclass(frozen=True)
class Comparator:
    operator: str
    version: Version

    def allows(self, version: Version) -> bool:
        match self.operator:
            case "=":
                return version == self.version
            case "!=":
                return version != self.version
            case ">":
                return version > self.version
            case ">=":
                return version >= self.version
            case "<":
                return version < self.version
            case "<=":
                return version <= self.version
        raise AssertionError(self.operator)

    def __str__(self) -> str:
        return f"{self.operator}{self.version}"


@dataclass(frozen=True)
class Constraint:
    text: str
    comparators: tuple[Comparator, ...]

    @classmethod
    def parse(cls, text: str, scheme: str = NUMERIC) -> "Constraint":
        if not isinstance(text, str):
            raise VersionError(f"version constraint must be a string, got {type(text).__name__} {text!r}")

        stripped = text.strip()
        if stripped == "*":
            return cls(text="*", comparators=())
        if not stripped:
            raise VersionError("version constraint is empty; use * to allow any version")

        comparators = []
        for part in stripped.split(","):
            part = part.strip()
            match = COMPARATOR_PATTERN.match(part)
            if not match:
                hint = ""
                if PATTERNS[DECIMAL].match(part):
                    hint = f" (write ={part} for exactly that version, or >={part} for it and newer)"
                elif part.startswith(("^", "~")):
                    hint = " (^ and ~ are not supported; write the range out, e.g. >=1.2, <2)"
                elif "||" in part:
                    hint = " (|| is not supported; constraints are AND-only)"
                raise VersionError(f"{text!r}: {part!r} is not a comparator{hint}")
            comparators.append(Comparator(match.group("operator"), Version.parse(match.group("version"), scheme)))

        normalized = ", ".join(str(comparator) for comparator in comparators)
        return cls(text=normalized, comparators=tuple(comparators))

    def allows(self, version: Version) -> bool:
        return all(comparator.allows(version) for comparator in self.comparators)

    def __str__(self) -> str:
        return self.text
