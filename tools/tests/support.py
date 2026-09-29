"""Shared test helpers: a throwaway copy of this template with its own git history."""

import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

# The tests build throwaway repositories, never the one CI is running in. A runner's GITHUB_*
# variables describe that repository, and some are channels back into the run: GITHUB_STEP_SUMMARY,
# GITHUB_OUTPUT, GITHUB_ENV. Left in place, GITHUB_REPOSITORY trips the repository check in every
# scratch build, GITHUB_ACTIONS turns expected warnings into annotations, and a scratch site's
# migration suggestions land in the real run's summary.
RUNNER_PREFIXES = ("GITHUB_", "DREAMWEAVE_")
for runner_variable in [name for name in os.environ if name.startswith(RUNNER_PREFIXES)]:
    os.environ.pop(runner_variable)


def scratch_environment() -> dict[str, str]:
    """The environment for commands run in a scratch repository, whatever a test has set since."""
    return {name: value for name, value in os.environ.items() if not name.startswith(RUNNER_PREFIXES)}

TEMPLATE_PARTS = ("templates", "sass", "static", "buildSite", "tools")
SITE_CONFIG = """
base_url = "https://example.github.io/cool-mods"
title = "Cool Mods"
compile_sass = true
build_search_index = true

[search]
index_format = "fuse_json"

[markdown]
insert_anchor_links = "right"

[extra]
github_username = "someone"
github_project = "cool-mods"
"""

# Sites made from the template may drop the network page, so the tests bring their own.
NETWORK_PAGE = """+++
title = "Network"
template = "dreamweave/network.html"

[extra]
comments = false
+++
What this site publishes.
"""


def git(root: Path, *arguments: str) -> str:
    process = subprocess.run(["git", *arguments], cwd=root, capture_output=True, text=True)
    if process.returncode != 0:
        raise AssertionError(f"git {' '.join(arguments)} failed: {process.stderr}")
    return process.stdout.strip()


def build_site(root: Path, *arguments: str, check: bool = True, env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
    process = subprocess.run([sys.executable, str(root / "buildSite"), *arguments], cwd=root, capture_output=True, text=True, env={**scratch_environment(), **(env or {})})
    if check and process.returncode != 0:
        raise AssertionError(f"buildSite {' '.join(arguments)} failed:\n{process.stdout}\n{process.stderr}")
    return process


class Scratch:
    """A site built from this repository's templates and tooling, with fixture content."""

    def __init__(self):
        self.directory = tempfile.TemporaryDirectory(prefix="dreamweave-test-")
        self.root = Path(self.directory.name)
        for part in TEMPLATE_PARTS:
            source = REPOSITORY / part
            if part == "static":
                shutil.copytree(source, self.root / part, ignore=shutil.ignore_patterns("dreamweave", "dreamweave.json", "processed_images"))
            elif source.is_dir():
                shutil.copytree(source, self.root / part, ignore=shutil.ignore_patterns("__pycache__"))
            else:
                shutil.copy2(source, self.root / part)
        (self.root / "config.toml").write_text(SITE_CONFIG)
        (self.root / ".gitignore").write_text((REPOSITORY / ".gitignore").read_text())
        self.write("content/_index.md", '+++\ntitle = "Cool Mods"\nsort_by = "title"\npaginate_by = 2\n+++\n')
        self.write("content/network.md", NETWORK_PAGE)
        git(self.root, "init", "-q", "-b", "main")
        git(self.root, "config", "user.email", "test@example.invalid")
        git(self.root, "config", "user.name", "Test")
        git(self.root, "config", "commit.gpgsign", "false")
        git(self.root, "config", "tag.gpgsign", "false")

    def write(self, relative: str, text: str) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(text).lstrip("\n"))
        return path

    def commit(self, message: str = "fixture") -> str:
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "--allow-empty", "-m", message)
        return git(self.root, "rev-parse", "HEAD")

    def add_project(self, directory: str, mod_toml: str, title: str = "Lantern", description: str = "Lights.", files: dict | None = None) -> None:
        self.write(f"content/{directory}/index.md", f'+++\ntitle = "{title}"\ndescription = "{description}"\n+++\nA mod.\n')
        self.write(f"content/{directory}/mod.toml", mod_toml)
        for name, text in (files or {}).items():
            self.write(f"content/{directory}/{name}", text)

    def cleanup(self) -> None:
        self.directory.cleanup()


LANTERN = """
id = "0b8f1c2d-3e4a-4b5c-8d6e-7f8091a2b3c4"
slug = "lantern"

[runtimes]
openmw = ">=0.49"

[openmw]
content_files = ["Lantern.omwscripts"]

[[releases]]
version = "1.0.0"
date = 2026-01-02
summary = "First."
"""

LANTERN_FILES = {
    "Lantern.omwscripts": "PLAYER: scripts/lantern/player.lua\n",
    "scripts/lantern/player.lua": "return {}\n",
}
