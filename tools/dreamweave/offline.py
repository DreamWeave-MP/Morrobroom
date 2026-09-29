"""Offline documentation: the project's rendered pages, shipped inside its archive.

Zola renders the site once in offline mode (no search, feeds, analytics, comments, clock or
distribution data), then each project's pages are crawled from its landing page. Links inside
the project become relative file paths, the stylesheets, fonts and images those pages use are
copied beside them, and links to anything else stay absolute so they still work online.

The render must be a function of the committed tree. Anything that reads mod.lock, git or the
clock would make the documentation change when the lock is written, and the lock records the
archive's hash. That loop is broken by the offline data file, which holds none of those.
"""

import datetime
import html as html_module
import re
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlsplit

SITE_ASSET_ROOT = "_site"
ZOLA_VERSION = "0.22.1"
TAG_PATTERN = re.compile(r"<[a-zA-Z][^>]*>")
ONLINE_MARKER = "data-dw-online"
ATTRIBUTE_PATTERN = re.compile(r'(?P<name>\b(?:href|src|poster))="(?P<value>[^"]*)"')
SRCSET_PATTERN = re.compile(r'\bsrcset="(?P<value>[^"]*)"')
CSS_URL_PATTERN = re.compile(r"url\((?P<quote>['\"]?)(?P<value>[^'\")]+)(?P=quote)\)")


class OfflineError(RuntimeError):
    pass


def zola_version() -> str | None:
    zola = shutil.which("zola")
    if not zola:
        return None
    output = subprocess.run([zola, "--version"], capture_output=True, text=True).stdout.strip()
    return output.removeprefix("zola ").strip() or None


def require_zola(pinned: bool) -> None:
    version = zola_version()
    if version is None:
        raise OfflineError("Zola is not installed; it renders the documentation that ships inside archives. https://www.getzola.org/documentation/getting-started/installation/")
    if pinned and version != ZOLA_VERSION:
        raise OfflineError(
            f"archives embed Zola-rendered documentation, so recording a release needs the Zola CI uses ({ZOLA_VERSION}); "
            f"this machine has {version}. Different renderers produce different bytes and CI would reject the tag."
        )


def toml_value(value: object) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return repr(value)
    if isinstance(value, str):
        escaped = value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")
        return f'"{escaped}"'
    if isinstance(value, (datetime.date, datetime.datetime)):
        return value.isoformat()
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{toml_key(key)} = {toml_value(item)}" for key, item in value.items()) + " }"
    raise TypeError(f"cannot write {type(value).__name__} to TOML")


def toml_key(key: str) -> str:
    return key if re.match(r"^[A-Za-z0-9_-]+$", key) else toml_value(key)


def toml_document(data: dict) -> str:
    """Enough TOML to re-emit a Zola config: scalars, arrays and inline tables under [sections]."""
    lines = []
    tables = []
    for key, value in data.items():
        if isinstance(value, dict):
            tables.append((key, value))
        else:
            lines.append(f"{toml_key(key)} = {toml_value(value)}")
    for name, table in tables:
        lines.append("")
        lines.append(f"[{toml_key(name)}]")
        for key, value in table.items():
            lines.append(f"{toml_key(key)} = {toml_value(value)}")
    return "\n".join(lines) + "\n"


def render_offline_site(root: Path, workspace: Path) -> Path:
    config = tomllib.loads((root / "config.toml").read_text(encoding="utf-8"))
    config["build_search_index"] = False
    config["generate_feeds"] = False
    config.pop("feed_filenames", None)
    extra = dict(config.get("extra", {}))
    extra["dreamweave_offline"] = True
    config["extra"] = extra

    config_path = workspace / "offline.toml"
    config_path.write_text(toml_document(config), encoding="utf-8")
    output = workspace / "public"
    process = subprocess.run(
        ["zola", "--root", str(root), "--config", str(config_path), "build", "--output-dir", str(output), "--force"],
        capture_output=True,
        text=True,
    )
    if process.returncode != 0:
        raise OfflineError(f"the offline documentation build failed:\n{process.stderr.strip() or process.stdout.strip()}")
    return output


class DocumentationCrawler:
    def __init__(self, output: Path, base_url: str, page_path: str):
        self.output = output
        self.base = urlsplit(base_url.rstrip("/") + "/")
        self.page_path = page_path
        self.files: dict[str, bytes] = {}
        self.pending: list[str] = []
        self.visited: set[str] = set()

    def site_path(self, reference: str, current_site_path: str) -> str | None:
        """The site-relative path a reference points at, or None if it leaves the site."""
        parts = urlsplit(reference)
        if parts.scheme in ("mailto", "javascript", "data", "tel") or reference.startswith("#"):
            return None
        if parts.scheme or parts.netloc:
            if parts.netloc != self.base.netloc or not parts.path.startswith(self.base.path):
                return None
            path = parts.path[len(self.base.path):]
        elif parts.path.startswith("/"):
            if not parts.path.startswith(self.base.path):
                return None
            path = parts.path[len(self.base.path):]
        else:
            directory = str(PurePosixPath(current_site_path).parent)
            path = str(PurePosixPath(directory) / parts.path) if directory != "." else parts.path
            path = normalize(path)
            if path is None:
                return None
        return unquote(path)

    def resolve_file(self, site_path: str) -> str | None:
        candidate = self.output / site_path
        if site_path.endswith("/") or site_path == "" or candidate.is_dir():
            candidate = candidate / "index.html"
            site_path = str(PurePosixPath(site_path) / "index.html") if site_path not in ("", "/") else "index.html"
        if candidate.is_file():
            return site_path
        return None

    def archive_path(self, site_path: str) -> str:
        if site_path.startswith(self.page_path):
            return site_path[len(self.page_path):]
        return f"{SITE_ASSET_ROOT}/{site_path}"

    def is_project_page(self, site_path: str) -> bool:
        return site_path.startswith(self.page_path) and site_path.endswith(".html")

    def localize(self, reference: str, current_site_path: str) -> str:
        reference = html_module.unescape(reference)
        parts = urlsplit(reference)
        target = self.site_path(reference, current_site_path)
        if target is None:
            return reference
        resolved = self.resolve_file(target)
        if resolved is None:
            return reference
        if resolved.endswith(".html") and not self.is_project_page(resolved):
            return f"{self.base.scheme}://{self.base.netloc}{self.base.path}{target}" + (f"#{parts.fragment}" if parts.fragment else "")

        self.enqueue(resolved)
        here = PurePosixPath(self.archive_path(current_site_path)).parent
        there = PurePosixPath(self.archive_path(resolved))
        relative = relative_path(there, here)
        return relative + (f"#{parts.fragment}" if parts.fragment else "")

    def enqueue(self, site_path: str) -> None:
        if site_path not in self.visited:
            self.visited.add(site_path)
            self.pending.append(site_path)

    def crawl(self) -> dict[str, bytes]:
        start = self.resolve_file(self.page_path)
        if start is None:
            raise OfflineError(f"the offline build has no page at {self.page_path}")
        self.enqueue(start)
        while self.pending:
            site_path = self.pending.pop(0)
            data = (self.output / site_path).read_bytes()
            if site_path.endswith(".html"):
                data = self.rewrite_html(data.decode("utf-8"), site_path).encode("utf-8")
            elif site_path.endswith(".css"):
                data = self.rewrite_css(data.decode("utf-8"), site_path).encode("utf-8")
            self.files[self.archive_path(site_path)] = data
        return self.files

    def rewrite_tag(self, tag: str, site_path: str) -> str:
        """Localize one start tag's URLs. A tag marked data-dw-online keeps pointing at the live site."""
        if ONLINE_MARKER in tag:
            return tag
        tag = ATTRIBUTE_PATTERN.sub(lambda match: f'{match.group("name")}="{escape_attribute(self.localize(match.group("value"), site_path))}"', tag)
        return SRCSET_PATTERN.sub(lambda match: f'srcset="{escape_attribute(self.rewrite_srcset(html_module.unescape(match.group("value")), site_path))}"', tag)

    def rewrite_html(self, html: str, site_path: str) -> str:
        html = TAG_PATTERN.sub(lambda match: self.rewrite_tag(match.group(0), site_path), html)
        return CSS_URL_PATTERN.sub(lambda match: f"url({match.group('quote')}{self.localize(match.group('value'), site_path)}{match.group('quote')})", html)

    def rewrite_srcset(self, value: str, site_path: str) -> str:
        candidates = []
        for candidate in value.split(","):
            pieces = candidate.strip().split()
            if pieces:
                pieces[0] = self.localize(pieces[0], site_path)
                candidates.append(" ".join(pieces))
        return ", ".join(candidates)

    def rewrite_css(self, css: str, site_path: str) -> str:
        return CSS_URL_PATTERN.sub(lambda match: f"url({match.group('quote')}{self.localize(match.group('value'), site_path)}{match.group('quote')})", css)


def escape_attribute(value: str) -> str:
    return html_module.escape(value, quote=True)


def normalize(path: str) -> str | None:
    segments = []
    for segment in path.split("/"):
        if segment == "..":
            if not segments:
                return None
            segments.pop()
        elif segment not in (".", ""):
            segments.append(segment)
    trailing = "/" if path.endswith("/") else ""
    return "/".join(segments) + trailing if segments else trailing


def relative_path(target: PurePosixPath, directory: PurePosixPath) -> str:
    target_parts = [part for part in target.parts if part != "."]
    directory_parts = [part for part in directory.parts if part != "."]
    common = 0
    while common < len(target_parts) - 1 and common < len(directory_parts) and target_parts[common] == directory_parts[common]:
        common += 1
    return "/".join([".."] * (len(directory_parts) - common) + target_parts[common:]) or "."


def build_documentation(root: Path, base_url: str, page_paths: list[str]) -> dict[str, dict[str, bytes]]:
    """Render once, then return each project's documentation files, relative to their folder, keyed by page path."""
    if not page_paths:
        return {}
    with tempfile.TemporaryDirectory(prefix="dreamweave-offline-") as workspace:
        output = render_offline_site(root, Path(workspace))
        return {page_path: DocumentationCrawler(output, base_url, page_path).crawl() for page_path in page_paths}
