"""The release lifecycle end to end, in a throwaway repository built from this template."""

import hashlib
import html
import json
import re
import shutil
import subprocess
import unittest
import zipfile
from pathlib import Path
from xml.etree import ElementTree

from support import LANTERN, LANTERN_FILES, REPOSITORY, Scratch, build_site, git

try:
    import jsonschema
except ImportError:
    jsonschema = None

LANTERN_ID = "0b8f1c2d-3e4a-4b5c-8d6e-7f8091a2b3c4"
HEARTH_ID = "5c6d7e8f-9a0b-4c1d-8e2f-3a4b5c6d7e8f"
HAS_ZOLA = shutil.which("zola") is not None

# A fomod project that leaves most things to their defaults, which Python and the templates must
# fill in the same way.
HEARTH = """
id = "5c6d7e8f-9a0b-4c1d-8e2f-3a4b5c6d7e8f"
slug = "hearth"

[runtimes]
openmw = "*"

[package]
format = "fomod"

[[components]]
id = "core"
name = "Core"
path = "00 Core"
required = true

[components.openmw]
content_files = ["Hearth.omwscripts"]

[[groups]]
id = "smoke"
name = "Smoke & <sparks>"
select = "exactly-one"

[[components]]
id = "light-smoke"
name = "Light smoke"
path = "10 Light"
group = "smoke"
default = true

[[components]]
id = "heavy-smoke"
name = "Heavy smoke"
path = "11 Heavy"
group = "smoke"

[[groups]]
id = "extras"
name = "Extras"

[[components]]
id = "embers"
name = "Embers"
path = "20 Embers"
group = "extras"
"""
BROOM_ID = "7d8e9f0a-1b2c-4d3e-8f4a-5b6c7d8e9f0a"
BROOM = """
id = "7d8e9f0a-1b2c-4d3e-8f4a-5b6c7d8e9f0a"
slug = "broom"
type = "tool"

[package]
format = "binary"
binary = "broom"
include = ["README.md"]

[[platforms]]
os = "linux"
arch = "x86_64"

[[platforms]]
os = "windows"
arch = "x86_64"

[[releases]]
version = "1.0.0"
date = 2026-01-02
summary = "First."
"""
HEARTH_FILES = {"00 Core/Hearth.omwscripts": "PLAYER: x.lua\n", "10 Light/a.txt": "a", "11 Heavy/b.txt": "b", "20 Embers/c.txt": "c"}


def load(root: Path, relative: str) -> dict:
    return json.loads((root / relative).read_text())


def schema_errors(document: dict, schema_name: str) -> list[str]:
    schema = json.loads((REPOSITORY / "static/schemas" / schema_name).read_text())
    validator = jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker())
    return [f"{list(error.path)}: {error.message}" for error in validator.iter_errors(document)]


@unittest.skipUnless(HAS_ZOLA, "Zola renders the documentation inside archives")
class ReleaseLifecycle(unittest.TestCase):
    def setUp(self):
        self.scratch = Scratch()
        self.root = self.scratch.root
        self.scratch.add_project("lantern", LANTERN, files=LANTERN_FILES)
        self.scratch.commit("Add Lantern")

    def tearDown(self):
        self.scratch.cleanup()

    def manifest(self) -> dict:
        return load(self.root, f"static/dreamweave/projects/{LANTERN_ID}.json")

    def release(self, version: str = "1.0.0") -> str:
        """What CI does when a tag is pushed: build at the tag, then record it on the default branch."""
        tag = f"lantern-{version}"
        git(self.root, "tag", "-f", tag)
        revision = git(self.root, "rev-parse", "HEAD")
        git(self.root, "checkout", "-q", tag)
        build_site(self.root, "release", tag)
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        self.scratch.commit(f"RELEASE: Lantern {version}")
        return revision

    def test_before_any_release_only_the_development_channel_exists(self):
        build_site(self.root, "build")
        manifest = self.manifest()
        self.assertEqual(list(manifest["channels"]), ["development"])
        development = manifest["releases"][0]
        self.assertEqual(development["channel"], "development")
        self.assertRegex(development["version"], r"^0\.0\.1-dev\.\d+$")
        artifact = development["artifacts"][0]
        self.assertEqual(artifact["sources"][0]["url"], "https://github.com/someone/cool-mods/releases/download/development/lantern.zip")
        self.assertEqual(artifact["digests"]["sha256"], hashlib.sha256((self.root / "dist/lantern.zip").read_bytes()).hexdigest())

    def test_the_template_publishes_development_builds_as_dev_build(self):
        # `development` is burned in the template's own repository: it was once an immutable release.
        build_site(self.root, "build")
        self.assertEqual((self.root / "dist/github-release").read_text(), "development\n")
        config = self.root / "config.toml"
        config.write_text(config.read_text().replace('github_username = "someone"', 'github_username = "DreamWeave-MP"').replace('github_project = "cool-mods"', 'github_project = "DreamWeave-Mod-Template"'))
        self.scratch.commit("Pretend to be the template")
        build_site(self.root, "build")
        self.assertEqual((self.root / "dist/github-release").read_text(), "dev-build\n")
        development = self.manifest()["releases"][0]
        self.assertEqual(development["source"]["release"], "dev-build")
        self.assertEqual(development["artifacts"][0]["sources"][0]["url"], "https://github.com/DreamWeave-MP/DreamWeave-Mod-Template/releases/download/dev-build/lantern.zip")

    def test_a_pushed_tag_is_built_recorded_and_published(self):
        revision = self.release()
        built = hashlib.sha256((self.root / "dist/lantern.zip").read_bytes()).hexdigest()
        lock = load(self.root, "content/lantern/mod.lock")
        self.assertEqual(lock["project"], LANTERN_ID)
        locked = lock["releases"][0]
        self.assertEqual(locked["version"], "1.0.0")
        self.assertEqual(locked["locked_from"], revision)
        self.assertEqual(locked["artifacts"][0]["digests"]["sha256"], built)
        self.assertIn("## Lantern 1.0.0", (self.root / "dist/release-notes.md").read_text())
        self.assertEqual((self.root / "dist/github-release").read_text(), "lantern-1.0.0\n")
        self.assertIn("Documentation/changelog/index.html", zipfile.ZipFile(self.root / "dist/lantern.zip").namelist())

        git(self.root, "checkout", "-q", "lantern-1.0.0")
        build_site(self.root, "release", "lantern-1.0.0")
        git(self.root, "checkout", "-q", "main")
        self.assertIn("matches its record", build_site(self.root, "record").stdout, "re-running a tag's job is harmless")
        self.assertEqual(git(self.root, "status", "--porcelain"), "")

        build_site(self.root, "build")
        manifest = self.manifest()
        self.assertEqual(manifest["channels"]["stable"], {"version": "1.0.0"})
        stable = next(release for release in manifest["releases"] if release["version"] == "1.0.0")
        self.assertEqual(stable["status"], "available")
        self.assertEqual(stable["source"]["revision"], revision)
        self.assertEqual(stable["source"]["tag"], "lantern-1.0.0")
        self.assertEqual(stable["artifacts"][0]["digests"], locked["artifacts"][0]["digests"])
        self.assertEqual(stable["artifacts"][0]["sources"][0]["url"], "https://github.com/someone/cool-mods/releases/download/lantern-1.0.0/lantern.zip")
        self.assertEqual(stable["notes"]["summary"], "First.")
        development = next(release for release in manifest["releases"] if release["channel"] == "development")
        self.assertEqual(development["version"], "1.0.1-dev.0", "CI's record commit changes nothing an archive contains")

        if jsonschema:
            self.assertEqual(schema_errors(manifest, "modManifest-2.schema.json"), [])
            self.assertEqual(schema_errors(load(self.root, "static/dreamweave.json"), "dreamweave-index-2.schema.json"), [])

    def test_extension_data_reaches_the_manifest_unchanged(self):
        mod_toml = (self.root / "content/lantern/mod.toml").read_text()
        extension = '[extensions."org.tes3mp"]\nserver_side = true\nsync = ["time", "weather"]\n\n'
        (self.root / "content/lantern/mod.toml").write_text(mod_toml.replace("[[releases]]", extension + "[[releases]]", 1))
        self.scratch.commit("Add a third-party extension")
        build_site(self.root, "build")
        release = self.manifest()["releases"][0]
        self.assertEqual(release["extensions"]["org.tes3mp"], {"server_side": True, "sync": ["time", "weather"]})
        self.assertEqual(release["critical_extensions"], ["openmw"])
        self.assertIn("server_side = true", (self.root / "content/lantern/mod.toml").read_text())

    def test_a_lock_that_belongs_to_another_project_is_refused(self):
        self.release()
        lock_path = self.root / "content/lantern/mod.lock"
        lock = json.loads(lock_path.read_text())
        lock["project"] = "11111111-2222-4333-8444-555555555555"
        lock_path.write_text(json.dumps(lock))
        self.scratch.commit("Tamper with the lock")
        process = build_site(self.root, "check", check=False)
        self.assertIn("claims someone else's releases", process.stdout + process.stderr)

    def test_the_repository_documents_match_their_schemas(self):
        if jsonschema is None:
            self.skipTest("jsonschema is not installed")
        self.release()
        build_site(self.root, "build")
        self.assertIn("match their schemas", build_site(self.root, "schemas").stdout)

    def add_broom(self) -> None:
        """A program, built per platform by the Rust workflow rather than zipped from content/."""
        self.scratch.write("README.md", "Broom sweeps.\n")
        self.scratch.add_project("broom", BROOM, title="Broom", description="Sweeps maps.")
        self.scratch.commit("Add Broom")

    def stage_binaries(self, build: str, platforms=("Linux-X64", "Windows-X64")) -> None:
        """What the workflow's download step leaves in dist/binaries/."""
        binaries = self.root / "dist/binaries"
        binaries.mkdir(parents=True, exist_ok=True)
        for platform in platforms:
            (binaries / f"broom-{platform}.zip").write_bytes(f"broom {platform} {build}".encode())

    def test_a_binary_release_records_every_platform_the_rust_workflow_built(self):
        self.add_broom()
        git(self.root, "tag", "broom-1.0.0")
        git(self.root, "checkout", "-q", "broom-1.0.0")
        process = build_site(self.root, "release", "broom-1.0.0", check=False)
        self.assertIn("broom-Linux-X64.zip", process.stderr, "without the Rust workflow's archives there is nothing to release")
        self.stage_binaries("1.0.0")
        build_site(self.root, "release", "broom-1.0.0")
        self.assertTrue((self.root / "dist/broom-Windows-X64.zip").is_file(), "the hashed bytes are the published bytes")
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        self.scratch.commit("RELEASE: Broom 1.0.0")

        locked = load(self.root, "content/broom/mod.lock")["releases"][0]
        self.assertEqual([artifact["id"] for artifact in locked["artifacts"]], ["linux-x64", "windows-x64"])
        windows = locked["artifacts"][1]
        self.assertEqual(windows["format"], "binary")
        self.assertEqual(windows["platform"], {"os": "windows", "arch": "x86_64"})
        self.assertEqual(windows["digests"]["sha256"], hashlib.sha256(b"broom Windows-X64 1.0.0").hexdigest())

        self.stage_binaries("dev")
        build_site(self.root, "build")
        manifest = load(self.root, f"static/dreamweave/projects/{BROOM_ID}.json")
        self.assertEqual(manifest["channels"]["stable"], {"version": "1.0.0"})
        development = next(release for release in manifest["releases"] if release["channel"] == "development")
        self.assertEqual(development["artifacts"][0]["sources"][0]["url"], "https://github.com/someone/cool-mods/releases/download/development/broom-Linux-X64.zip")
        self.assertEqual(development["artifacts"][0]["digests"]["sha256"], hashlib.sha256(b"broom Linux-X64 dev").hexdigest())
        if jsonschema:
            self.assertEqual(schema_errors(manifest, "modManifest-2.schema.json"), [])

    def test_a_programs_page_offers_every_platform(self):
        self.add_broom()
        git(self.root, "tag", "broom-1.0.0")
        self.stage_binaries("1.0.0")
        git(self.root, "checkout", "-q", "broom-1.0.0")
        build_site(self.root, "release", "broom-1.0.0")
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        self.scratch.commit("RELEASE: Broom 1.0.0")
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = html.unescape((self.root / "public/broom/index.html").read_text())
        hero = re.search(r'<div class="dw-actions dw-platforms".*?</div>', page, re.S).group(0)
        self.assertIn('data-platform="windows" href="https://github.com/someone/cool-mods/releases/download/broom-1.0.0/broom-Windows-X64.zip"', hero)
        self.assertIn(">Linux <", hero)
        self.assertNotIn("Mod manager", page, "a program is not handed to a mod manager")
        self.assertNotIn("OpenMW, by hand", page)
        self.assertIn("broom-&lt;platform&gt;.zip", (self.root / "public/broom/index.html").read_text())
        self.assertIn("<dt>Package</dt><dd>Program <small>2 platforms</small>", page)

    def test_the_rust_workflow_is_told_what_to_build(self):
        self.assertEqual(build_site(self.root, "binaries").stdout, "binary_names=[]\ninclude_files=\n")
        self.add_broom()
        self.assertEqual(build_site(self.root, "binaries").stdout, 'binary_names=["broom"]\ninclude_files=README.md\n')

    def test_a_binary_project_without_its_build_has_no_development_channel(self):
        self.add_broom()
        output = build_site(self.root, "build").stdout
        self.assertIn("broom has no development build", output)
        self.assertEqual(load(self.root, f"static/dreamweave/projects/{BROOM_ID}.json")["channels"], {})

    def test_a_published_release_never_changes(self):
        self.release()
        self.scratch.write("content/lantern/scripts/lantern/player.lua", "return { changed = true }\n")
        self.scratch.commit("Change the mod after releasing it")
        git(self.root, "tag", "-f", "lantern-1.0.0")
        git(self.root, "checkout", "-q", "lantern-1.0.0")
        build_site(self.root, "release", "lantern-1.0.0")
        git(self.root, "checkout", "-q", "main")
        process = build_site(self.root, "record", check=False)
        self.assertNotEqual(process.returncode, 0)
        self.assertIn("is not the release already recorded", process.stderr)
        self.assertIn("Declare the next version", process.stderr)

    def test_a_tag_needs_a_project_and_a_declared_release(self):
        git(self.root, "tag", "lantern-2.0.0")
        git(self.root, "tag", "lamp-1.0.0")
        git(self.root, "checkout", "-q", "lantern-2.0.0")
        process = build_site(self.root, "release", "lantern-2.0.0", check=False)
        self.assertIn("has no [[releases]] entry for 2.0.0", process.stderr)
        process = build_site(self.root, "release", "lamp-1.0.0", check=False)
        self.assertIn("does not name a project", process.stderr)
        self.assertIn("lantern-<version>", process.stderr)
        git(self.root, "checkout", "-q", "main")

    def test_a_release_is_recorded_only_where_it_is_declared(self):
        git(self.root, "checkout", "-q", "-b", "next")
        mod_toml = (self.root / "content/lantern/mod.toml").read_text()
        (self.root / "content/lantern/mod.toml").write_text(mod_toml + '\n[[releases]]\nversion = "1.1.0"\ndate = 2026-02-01\n')
        self.scratch.commit("Declare 1.1.0 on a branch")
        git(self.root, "tag", "lantern-1.1.0")
        build_site(self.root, "release", "lantern-1.1.0")
        git(self.root, "checkout", "-q", "main")
        process = build_site(self.root, "record", check=False)
        self.assertIn("does not declare 1.1.0", process.stderr)
        self.assertIn("Merge the tagged commit", process.stderr)
        self.assertFalse((self.root / "content/lantern/mod.lock").exists())

    def test_packaging_is_byte_reproducible(self):
        build_site(self.root, "build")
        first = (self.root / "dist/lantern.zip").read_bytes()
        shutil.rmtree(self.root / "dist")
        build_site(self.root, "build")
        self.assertEqual(first, (self.root / "dist/lantern.zip").read_bytes())

    def test_yanked_releases_stay_listed_but_leave_the_channel(self):
        self.release("1.0.0")
        mod_toml = (self.root / "content/lantern/mod.toml").read_text()
        (self.root / "content/lantern/mod.toml").write_text(mod_toml + '\n[[releases]]\nversion = "1.1.0"\ndate = 2026-02-01\n')
        self.scratch.commit("Declare 1.1.0")
        self.release("1.1.0")
        mod_toml = (self.root / "content/lantern/mod.toml").read_text()
        (self.root / "content/lantern/mod.toml").write_text(mod_toml.replace('version = "1.1.0"\n', 'version = "1.1.0"\nyanked = "Deletes saves."\nreplacement = "1.0.0"\n'))
        self.scratch.commit("Yank 1.1.0")

        build_site(self.root, "build")
        manifest = self.manifest()
        yanked = next(release for release in manifest["releases"] if release["version"] == "1.1.0")
        self.assertEqual(yanked["status"], "yanked")
        self.assertEqual(yanked["yanked"], {"reason": "Deletes saves.", "replacement": "1.0.0"})
        self.assertEqual(manifest["channels"]["stable"], {"version": "1.0.0"})

    def test_archive_contents(self):
        build_site(self.root, "build")
        archive = zipfile.ZipFile(self.root / "dist/lantern.zip")
        names = set(archive.namelist())
        for expected in ("Lantern.omwscripts", "scripts/lantern/player.lua", "index.md", "mod.toml", "dreamweave.release.json", "Documentation/index.html"):
            self.assertIn(expected, names)
        self.assertNotIn("mod.lock", names)
        self.assertFalse(any(name.startswith("_changelog") for name in names))

        release = json.loads(archive.read("dreamweave.release.json"))
        self.assertEqual(release["project"]["id"], LANTERN_ID)
        if jsonschema:
            self.assertEqual(schema_errors(release, "dreamweave-release-payload-2.schema.json"), [])

        page = archive.read("Documentation/index.html").decode()
        project_links = re.findall(r'(?:href|src)="https://example\.github\.io/cool-mods/lantern/[^"]*"', page)
        self.assertEqual(len(project_links), 1, f"only the offline banner's live-page link may stay absolute: {project_links}")
        self.assertIn('data-dw-online', page)
        self.assertIn('href="changelog/index.html"', page)
        for reference in re.findall(r'(?:href|src)="(_site/[^"#]+)"', page):
            self.assertIn(f"Documentation/{reference}", names, f"offline page references a file the archive lacks: {reference}")
        for info in archive.infolist():
            self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
            self.assertEqual(info.compress_type, zipfile.ZIP_STORED)

    def test_fomod_installer_matches_the_component_model(self):
        self.scratch.add_project("hearth", HEARTH, title="Hearth", files=HEARTH_FILES)
        self.scratch.commit("Add Hearth")
        build_site(self.root, "build")
        archive = zipfile.ZipFile(self.root / "dist/hearth.zip")
        config = ElementTree.fromstring(archive.read("fomod/ModuleConfig.xml"))
        self.assertEqual(config.findtext("moduleName"), "Hearth")
        self.assertEqual([folder.get("source") for folder in config.find("requiredInstallFiles")], ["00 Core"])
        group = config.find(".//group")
        self.assertEqual(group.get("name"), "Smoke & <sparks>")
        self.assertEqual(group.get("type"), "SelectExactlyOne")
        types = {plugin.get("name"): plugin.find(".//type").get("name") for plugin in group.iter("plugin")}
        self.assertEqual(types, {"Light smoke": "Recommended", "Heavy smoke": "Optional"})
        ElementTree.fromstring(archive.read("fomod/info.xml"))

    def test_rendered_site_advertises_discovery_under_a_subdirectory(self):
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = (self.root / "public/lantern/index.html").read_text()
        self.assertIn('type="application/vnd.dreamweave.index+json" title="DreamWeave index" href="https://example.github.io/cool-mods/dreamweave.json"', page)
        self.assertIn(f'href="https://example.github.io/cool-mods/dreamweave/projects/{LANTERN_ID}.json"', page)
        self.assertTrue((self.root / "public/dreamweave.json").is_file())
        self.assertTrue((self.root / f"public/dreamweave/projects/{LANTERN_ID}.json").is_file())
        self.assertTrue((self.root / "public/lantern/mod.toml").exists(), "mod.toml is served beside its page, so `zola serve` reloads when it changes")
        index = load(self.root, "public/dreamweave.json")
        self.assertEqual(index["projects"][0]["manifest"], f"https://example.github.io/cool-mods/dreamweave/projects/{LANTERN_ID}.json")
        self.assertEqual(index["projects"][0]["manifest_sha256"], hashlib.sha256((self.root / f"public/dreamweave/projects/{LANTERN_ID}.json").read_bytes()).hexdigest())

    def zola_build(self) -> str:
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        return (self.root / "public/lantern/index.html").read_text()

    def test_zola_alone_renders_a_project_page(self):
        # Authors preview with a plain `zola serve`; nothing generated by CI may be needed.
        self.release()
        shutil.rmtree(self.root / "static/dreamweave", ignore_errors=True)
        for stub in self.root.glob("content/**/_changelog.md"):
            stub.unlink()
        page = self.zola_build()
        locked = load(self.root, "content/lantern/mod.lock")["releases"][0]["artifacts"][0]
        self.assertIn('href="https://github.com/someone/cool-mods/releases/download/lantern-1.0.0/lantern.zip"', page)
        self.assertIn(locked["digests"]["sha256"], page)
        self.assertIn("content=Lantern.omwscripts", page)
        self.assertIn('id="v1-0-0"', page, "without the generated changelog page, the project page lists every release")
        self.assertNotIn("changelog/", page)

    def test_a_page_can_leave_sections_out(self):
        index = self.root / "content/lantern/index.md"
        index.write_text(index.read_text().replace('description = "Lights."', 'description = "Lights."\n[extra]\nsections = ["overview", "credits"]'))
        page = self.zola_build()
        self.assertNotIn('id="install"', page)
        self.assertNotIn('href="#install"', page, "no button may point at a section the page left out")
        strip = re.search(r'<dl class="dw-strip".*?</dl>', page, re.S).group(0)
        self.assertNotIn("<dt>Package</dt>", strip, "a page with nothing to install does not describe its package")

    def test_the_default_favicon_follows_the_palette(self):
        config = self.root / "config.toml"
        config.write_text(config.read_text().replace('[extra]\n', '[extra]\npalette = "teal"\n'))
        page = self.zola_build()
        self.assertIn('href="https://example.github.io/cool-mods/img/mark-teal.svg" type="image/svg+xml"', page)
        self.assertTrue((self.root / "public/img/mark-teal.svg").is_file())

    def test_a_leftover_offline_view_does_not_break_the_site(self):
        # An interrupted CI build leaves the offline documentation's view behind.
        self.scratch.write("static/dreamweave/view.json", '{"offline": true, "generator": "x", "projects": {"lantern/": {"packaged_version": "1.0.0"}}}')
        page = self.zola_build()
        self.assertNotIn("Offline documentation", page)
        self.assertIn("Distribution metadata", (self.root / "public/network/index.html").read_text())

    def test_the_page_says_what_the_manifest_says(self):
        self.scratch.add_project("hearth", HEARTH, title="Hearth", files=HEARTH_FILES)
        self.scratch.commit("Add Hearth")
        self.release()
        build_site(self.root, "build")
        self.zola_build()
        for project_id, directory in ((LANTERN_ID, "lantern"), (HEARTH_ID, "hearth")):
            manifest = load(self.root, f"static/dreamweave/projects/{project_id}.json")
            page = (self.root / f"public/{directory}/index.html").read_text()
            model = json.loads(re.search(r"data-install-model>(.*?)</script>", page, re.S).group(1))
            newest = manifest["releases"][0]
            self.assertEqual(
                [(component["id"], component["required"], component["default"], component.get("group")) for component in model["components"]],
                [(component["id"], component["required"], component["default"], component.get("group")) for component in newest["components"]],
            )
            for release in manifest["releases"]:
                self.assertIn(release["version"], page)
                for artifact in release["artifacts"]:
                    self.assertIn(artifact["digests"]["sha256"], page)
                    self.assertIn(f'href="{artifact["sources"][0]["url"]}"', page)
            for group in newest["groups"]:
                self.assertIn(f"{group['name']} · {group['select']}", html.unescape(page))

    def test_author_text_is_escaped(self):
        hostile = "O'Brien's </script><script>alert(1)</script>"
        index = (self.root / "content/lantern/index.md").read_text().replace('title = "Lantern"', f"title = {json.dumps(hostile)}")
        (self.root / "content/lantern/index.md").write_text(index)
        mod_toml = (self.root / "content/lantern/mod.toml").read_text().replace('summary = "First."', 'summary = "<img src=x onerror=alert(2)>"')
        (self.root / "content/lantern/mod.toml").write_text(mod_toml)
        self.scratch.commit("Hostile text")
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = (self.root / "public/lantern/index.html").read_text()
        self.assertNotIn("<script>alert(1)", page)
        self.assertNotIn("<img src=x onerror", page)
        self.assertNotIn("&amp;#x27;", page, "text must be escaped once, not twice")
        self.assertIn("O&#x27;Brien", page)
        install_model = re.search(r'<script type="application/json" data-install-model>(.*?)</script>', page, re.S).group(1)
        self.assertNotIn("<", install_model)
        self.assertEqual(json.loads(install_model)["name"], hostile)


if __name__ == "__main__":
    unittest.main()
