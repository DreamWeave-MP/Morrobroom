"""Suggested mod.toml files for V4 pages, written by CI's check for the author to review.

A V4 page without a mod.toml fails the check, and the same run writes a suggestion for it under
dist/migration/ (uploaded as an artifact, laid out like the repository) and into the run's
summary. Nothing here touches the repository itself: the author reviews and commits.

The slug keeps V4's title slug so existing <slug>-<version> tags stay this project's history.
Every data directory V4 listed stays installed, so the archive and install behave as before;
splitting them into optional components is a decision for the author, not for a converter.
"""

import datetime
import json
import os
import re
import subprocess
import textwrap
import uuid
from pathlib import Path

from .model import read_frontmatter
from .versions import DECIMAL, NUMERIC, Version, VersionError

LEGACY_EXTRA_KEYS = (
    "version", "install_info", "nexus_id", "nexus_group_id", "offsite_host", "is_binary", "game",
    "hide_download_bar", "stable_title", "dev_title", "show_only_description", "use_toc", "content_files",
)


def legacy_slug(title: str) -> str:
    text = re.sub(r"[\s\-]+", "_", title.lower().strip())
    text = re.sub(r"[^a-z0-9_]+", "_", text)
    return re.sub(r"_+", "_", text).strip("_")


def toml_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def toml_list(values: list[str]) -> str:
    return "[" + ", ".join(toml_string(value) for value in values) + "]"


def meaningful(values) -> list:
    return [value for value in (values or []) if not str(value).startswith("#")]


def tags_for(directory: Path, slug: str) -> list[tuple[str, str]]:
    """(version text, date) for every <slug>-<version> tag."""
    output = subprocess.run(
        ["git", "for-each-ref", "--format=%(refname:short) %(creatordate:short)", f"refs/tags/{slug}-*"],
        cwd=directory, capture_output=True, text=True,
    ).stdout
    return [(tag[len(slug) + 1:], date) for tag, date in (line.split(" ", 1) for line in output.splitlines())]


def ordering_violations(history: list[tuple[str, str]], scheme: str) -> list[str]:
    """Everything wrong with a history under one scheme: unparsable versions and misordered dates."""
    parsed, problems = [], []
    for text, date in history:
        try:
            parsed.append((Version.parse(text, scheme), date))
        except VersionError:
            problems.append(f"{text} is not a {scheme} version")
    ordered = sorted(parsed, key=lambda item: (item[1], item[0].precedence_key()))
    return problems + [
        f"{later} ({later_date}) sorts below {earlier} ({earlier_date})"
        for (earlier, earlier_date), (later, later_date) in zip(ordered, ordered[1:])
        if later < earlier
    ]


def suggest(directory: Path) -> str:
    """A mod.toml for a V4 page, with what the converter could not decide as comments on top."""
    body, notes = suggestion(directory)
    header = [f"# Suggested from the V4 frontmatter in {directory.name}/index.md. Review every line, then commit it as mod.toml."]
    for note in notes:
        header += textwrap.wrap(note, width=98, initial_indent="# - ", subsequent_indent="#   ")
    return "\n".join(header) + "\n" + body


def write_suggestions(root: Path, directories: list[Path]) -> list[Path]:
    """dist/migration/<page directory>/mod.toml for each V4 page, and the same in the run's summary."""
    written = []
    summary = ["## Suggested mod.toml files", "", "These pages still carry V4 frontmatter. Download the `mod-toml-suggestions` artifact and unzip it at the repository root, or copy from below. Review each file before committing it: content/guide/migration.md explains every line.", ""]
    for directory in directories:
        relative = directory.relative_to(root).as_posix()
        text = suggest(directory)
        path = root / "dist" / "migration" / relative / "mod.toml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        written.append(path)
        summary += [f"### {relative}/mod.toml", "", "```toml", text.rstrip("\n"), "```", ""]
    step_summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if written and step_summary:
        with open(step_summary, "a", encoding="utf-8") as handle:
            handle.write("\n".join(summary) + "\n")
    return written


def suggestion(directory: Path) -> tuple[str, list[str]]:
    frontmatter = read_frontmatter(directory / "index.md")
    if frontmatter is None:
        raise SystemExit(f"{directory}/index.md has no frontmatter.")
    extra = frontmatter.get("extra") or {}
    title = frontmatter.get("title")
    if not title:
        raise SystemExit(f"{directory}/index.md has no title.")

    candidates = [legacy_slug(title), legacy_slug(directory.name)]
    slug = next((candidate for candidate in candidates if tags_for(directory, candidate)), candidates[0])
    notes = []
    lines = [
        f"id = {toml_string(str(uuid.uuid4()))}",
        f"slug = {toml_string(slug)}  # matches the existing {slug}-<version> tags, so they stay this project's history",
    ]
    if extra.get("is_binary"):
        lines.append('type = "tool"')
    if extra.get("game") and extra["game"] != "morrowind":
        lines.append(f"game = {toml_string(extra['game'])}")

    offsite = extra.get("offsite_host")
    if offsite:
        lines += ["", "[links]"]
        if offsite.get("provider", "github") == "github":
            lines.append(f'source = "https://github.com/{offsite["owner"]}/{offsite["repo"]}"')
        notes.append(
            "offsite_host: V5 packages what is in this repository. A mod released from another repository "
            "should publish its own mod.toml and manifest there; this page can link to it instead."
        )

    install = extra.get("install_info") or {}
    data_directories = meaningful(install.get("data_directories"))
    content_files = meaningful(install.get("content_files")) or meaningful(extra.get("content_files"))
    fallback_entries = {key: value for key, value in (install.get("fallback_entries") or install.get("fallback") or {}).items() if not str(key).startswith("#")}
    config = meaningful(install.get("config"))

    lines += ["", "[runtimes]", 'openmw = "*"  # narrow this to the versions you have tested, e.g. ">=0.49"', "", "[openmw]"]
    if data_directories and data_directories != ["."]:
        lines.append(f"data_directories = {toml_list(data_directories)}")
    if content_files:
        lines.append(f"content_files = {toml_list(content_files)}")
    if config:
        if config != ["."]:
            notes.append(f"install_info.config listed {config}; V5 supports an openmw.cfg in the project root (config = true). Move the others or drop them.")
        lines.append("config = true")
    if fallback_entries:
        lines.append("")
        lines.append("[openmw.fallback_entries]")
        for key, value in fallback_entries.items():
            lines.append(f"{toml_string(key)} = {toml_string(str(value))}")
    if len(data_directories) > 1:
        notes.append(
            f"{len(data_directories)} data directories are all installed together, as in V4. To let players choose, "
            'switch to [package] format = "bain" or "fomod" and declare [[components]]; see the guide.'
        )

    if "nexus_id" in extra or "nexus_group_id" in extra:
        lines += ["", "[nexusmods]"]
        if "nexus_id" in extra:
            lines.append(f"mod_id = {int(extra['nexus_id'])}")
        if "nexus_group_id" in extra:
            lines.append(f"file_group_id = {toml_string(str(extra['nexus_group_id']))}")

    tags = tags_for(directory, slug)
    numeric_problems = ordering_violations(tags, NUMERIC)
    decimal_problems = ordering_violations(tags, DECIMAL)
    scheme = NUMERIC
    if numeric_problems and len(decimal_problems) < len(numeric_problems):
        scheme = DECIMAL
        lines.insert(2, 'versioning = "decimal"  # these releases were numbered like decimals: 0.82 comes before 0.9')

    history = []
    for text, date in tags:
        try:
            history.append((Version.parse(text, scheme), text, date))
        except VersionError:
            notes.append(f"tag {slug}-{text} is not a {scheme} version, so it is left out of [[releases]].")
    for problem in ordering_violations([(text, date) for _, text, date in history], scheme)[:3]:
        notes.append(f"historical tags are out of order ({problem}). That is only history: tags CI never recorded never enter the manifest.")

    current_text = str(extra.get("version", "")).strip()
    try:
        current = Version.parse(current_text, scheme) if current_text else None
    except VersionError:
        current = None
        notes.append(f"extra.version {current_text!r} is a placeholder, not a version; declare the first real release when it ships.")
    if current is not None and all(version != current for version, _, _ in history):
        newest = max((version for version, _, _ in history), default=None)
        if newest is None or current > newest:
            history.append((current, current_text, datetime.date.today().isoformat()))
            notes.append(f"{current_text} has no tag yet, so it is declared as the next release, dated today. Push its tag when it ships.")
        else:
            notes.append(f"extra.version {current_text} sorts below the newest tag {newest}; it was never bumped, so it is not carried over.")
    history = [(text, date) for _, text, date in history]

    for text, date in sorted(history, key=lambda item: item[1]):
        lines += ["", "[[releases]]", f"version = {toml_string(text)}", f"date = {date}"]
    if history:
        notes.append(
            "Tagged releases published before V5 have no recorded hash, so the manifest leaves them out and the "
            "changelog marks them unverified. Tag your next release to publish it to the network."
        )

    stale = [key for key in LEGACY_EXTRA_KEYS if key in extra]
    if stale:
        notes.append(f"Then delete these from index.md's [extra]: {', '.join(stale)}.")
    return "\n".join(lines) + "\n", notes
