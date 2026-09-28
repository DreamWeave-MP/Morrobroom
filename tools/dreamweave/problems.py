"""Validation problems, collected so one run reports everything wrong, not just the first thing."""

from dataclasses import dataclass, field


@dataclass
class Problems:
    errors: list[str] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def error(self, where: str, message: str) -> None:
        self.errors.append(f"{where}: {message}")

    def note(self, where: str, message: str) -> None:
        self.notes.append(f"{where}: {message}")

    def extend(self, other: "Problems") -> None:
        self.errors.extend(other.errors)
        self.notes.extend(other.notes)

    def raise_if_any(self) -> None:
        if self.errors:
            raise InvalidRepository(self)


class InvalidRepository(Exception):
    def __init__(self, problems: Problems):
        self.problems = problems
        super().__init__(f"{len(problems.errors)} problem(s)")

    def render(self) -> str:
        lines = [f"DreamWeave found {len(self.problems.errors)} problem(s):", ""]
        lines.extend(f"  - {error}" for error in self.problems.errors)
        return "\n".join(lines)
