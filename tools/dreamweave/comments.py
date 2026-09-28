"""Comments: GitHub Discussions on the site's own repository, embedded with giscus.

giscus needs the repository's and the category's GraphQL ids. V4 had authors paste them into
config.toml, and pasted ids are how St4sh's page threads ended up in the Mod Template's Discussions.
Now the ids are looked up from giscus for the repository config.toml names (which CI checks is the
repository it runs in), so a copied config can only ever post into its own repository.
"""

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

from .model import SiteConfig
from .problems import Problems
from .tables import Table

GISCUS_API = "https://giscus.app/api/discussions/categories"
MAPPINGS = ("pathname", "title", "og:title")


@dataclass
class CommentsSetting:
    enabled: bool
    category: str
    reactions: bool
    theme: str | None
    mapping: str


def read_comments_setting(site: SiteConfig, problems: Problems) -> CommentsSetting | None:
    if "giscus" in site.extra:
        problems.error(
            "config.toml [extra] giscus",
            "is the V4 comments setting, with pasted repository and category ids. V5 looks the ids up for this "
            "site's own repository, so a copied config cannot send comments to someone else's Discussions (St4sh's did). "
            "Replace it with [extra.comments]; see the guide's Customizing page",
        )
    if "comments" not in site.extra:
        return None

    table = Table(site.extra["comments"], "config.toml [extra.comments]", problems)
    setting = CommentsSetting(
        enabled=table.boolean("enabled", True),
        category=table.string("category", "General") or "General",
        reactions=table.boolean("reactions", False),
        theme=table.string("theme", None),
        mapping=table.choice("mapping", MAPPINGS, "pathname"),
    )
    table.finish()
    return setting if setting.enabled else None


def fetch_categories(repository: str) -> dict:
    query = urllib.parse.urlencode({"repo": repository})
    request = urllib.request.Request(f"{GISCUS_API}?{query}", headers={"User-Agent": "dreamweave-mod-template"})
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        try:
            return json.loads(error.read().decode("utf-8"))
        except (ValueError, OSError):
            return {"error": f"giscus answered HTTP {error.code}"}


def resolve_comments(site: SiteConfig, setting: CommentsSetting | None, problems: Problems, fetch=fetch_categories) -> dict:
    """What the templates need to embed comments, or why there are none.

    `state` is on, off, not-installed or unreachable. Only a category name that does not exist is an
    error: everything else about comments is optional and must not stop a mod from being published.
    """
    if setting is None:
        return {"state": "off"}

    repository = site.repository
    base = {"repo": repository, "category": setting.category, "reactions": setting.reactions, "theme": setting.theme, "mapping": setting.mapping}
    try:
        answer = fetch(repository)
    except (urllib.error.URLError, OSError, ValueError) as error:
        message = f"could not reach giscus to look up {repository}'s Discussions ({error}); comments are left out of this build"
        warn(message)
        return {**base, "state": "unreachable", "message": message}

    if "error" in answer:
        message = f"{repository}: {answer['error']}. Enable Discussions, install https://github.com/apps/giscus on the repository, and rebuild"
        warn(message)
        return {**base, "state": "not-installed", "message": message}

    categories = {category["name"]: category["id"] for category in answer.get("categories", [])}
    if setting.category not in categories:
        problems.error(
            "config.toml [extra.comments] category",
            f"{setting.category!r} is not a Discussions category of {repository}; it has {', '.join(sorted(categories)) or 'none'}",
        )
        return {**base, "state": "off"}

    return {**base, "state": "on", "repo_id": answer["repositoryId"], "category_id": categories[setting.category]}


def warn(message: str) -> None:
    if os.environ.get("GITHUB_ACTIONS") == "true":
        print(f"::warning title=Comments::{message}")
    else:
        print(f"warning: comments: {message}")
