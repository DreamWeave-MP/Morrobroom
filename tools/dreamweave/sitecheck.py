"""Check the built site: local links, assets and #fragments (also under a Pages subdirectory), and
that every page closes the block elements it opens.

Adapted from StroggForge's scripts/war-room/check-site.py. External links are not fetched: whether
GitHub is up says nothing about whether this site is correct.
"""

from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urljoin, urlsplit


# Elements whose end tag HTML requires. A template that opens one more than it closes produces a
# page browsers repair silently and differently; li and p are left out because their end tags are
# optional and hand-written content omits them legitimately.
BALANCED_ELEMENTS = {"div", "main", "section", "article", "aside", "header", "footer", "nav", "ul", "ol", "table", "details", "figure", "dl"}


class Document(HTMLParser):
    def __init__(self, text: str):
        super().__init__(convert_charrefs=True)
        self.ids: set[str] = set()
        self.links: list[str] = []
        self.open_elements: list[str] = []
        self.balance_errors: list[str] = []
        self.feed(text)
        if self.open_elements:
            self.balance_errors.append(f"never closed: {', '.join(self.open_elements[-3:])}")

    def handle_endtag(self, tag):
        if tag not in BALANCED_ELEMENTS:
            return
        if self.open_elements and self.open_elements[-1] == tag:
            self.open_elements.pop()
        else:
            self.balance_errors.append(f"</{tag}> closes {self.open_elements[-1] if self.open_elements else 'nothing'}")

    def handle_starttag(self, tag, attributes):
        if tag in BALANCED_ELEMENTS:
            self.open_elements.append(tag)
        attributes = dict(attributes)
        if "id" in attributes:
            self.ids.add(attributes["id"])
        if tag in ("a", "link") and attributes.get("href"):
            self.links.append(attributes["href"])
        if tag in ("img", "script", "source") and attributes.get("src"):
            self.links.append(attributes["src"])


def check_site(public: Path, base_url: str) -> tuple[int, list[str]]:
    base = base_url.rstrip("/") + "/"
    base_parts = urlsplit(base)
    documents = {path: Document(path.read_text(encoding="utf-8")) for path in sorted(public.rglob("*.html"))}
    if not documents:
        return 0, [f"{public} has no HTML; run zola build first"]

    errors = []
    checked = 0
    for path, document in documents.items():
        relative = path.relative_to(public).as_posix()
        errors.extend(f"{relative}: unbalanced HTML, {error}" for error in document.balance_errors[:2])
        current = urljoin(base, relative.removesuffix("index.html"))
        for link in document.links:
            target = urlsplit(urljoin(current, link))
            if target.scheme not in ("http", "https") or target.netloc != base_parts.netloc:
                continue
            if not target.path.startswith(base_parts.path):
                if not urlsplit(link).netloc:
                    errors.append(f"{relative}: {link} escapes the site's base path {base_parts.path}")
                continue
            local = public / unquote(target.path[len(base_parts.path):])
            if local.is_dir():
                local /= "index.html"
            checked += 1
            if not local.is_file():
                errors.append(f"{relative}: missing target {link}")
            elif target.fragment and local.suffix == ".html" and unquote(target.fragment) not in documents[local].ids:
                errors.append(f"{relative}: missing anchor {link}")
    return checked, errors


def check_protocol_documents(root: Path) -> tuple[int, list[str]]:
    """Validate the generated index and manifests against the schemas the site publishes."""
    try:
        import jsonschema
    except ImportError as error:
        raise SystemExit("./buildSite schemas needs jsonschema: python3 -m pip install -r tools/requirements.txt") from error
    import json

    schemas = root / "static" / "schemas"
    documents = [(root / "static" / "dreamweave.json", "dreamweave-index-2.schema.json")]
    documents += [(path, "modManifest-2.schema.json") for path in sorted((root / "static" / "dreamweave" / "projects").glob("*.json"))]
    errors = []
    for path, schema_name in documents:
        if not path.is_file():
            errors.append(f"{path.relative_to(root)} does not exist; run ./buildSite build first")
            continue
        schema = json.loads((schemas / schema_name).read_text(encoding="utf-8"))
        validator = jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker())
        for error in validator.iter_errors(json.loads(path.read_text(encoding="utf-8"))):
            location = "/".join(str(part) for part in error.absolute_path) or "(root)"
            errors.append(f"{path.relative_to(root)} {location}: {error.message}")
    return len(documents), errors
