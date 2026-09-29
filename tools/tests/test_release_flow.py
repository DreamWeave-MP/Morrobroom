"""The release lifecycle end to end, in a throwaway repository built from this template."""

import hashlib
import html
import json
import re
import shutil
import subprocess
import tempfile
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

[[platforms]]
os = "android"
arch = "aarch64"

[[platforms]]
os = "linux"
arch = "aarch64"
variant = "portmaster"

[[platforms]]
os = "linux"
arch = "aarch64"
variant = "muos"

[[releases]]
version = "1.0.0"
date = 2026-01-02
summary = "First."
"""
# What StroggForge's Rust workflow stages for Broom's platforms, in [[platforms]] order.
BROOM_ARCHIVES = ("Linux-X64.zip", "Windows-X64.zip", "Android-ARM64.zip", "Portmaster-ARM64.zip", "Portmaster-ARM64.muxapp")
LEDGER_ID = "9e0f1a2b-3c4d-4e5f-8a6b-7c8d9e0f1a2b"
# A Rust library: released to crates.io by StroggForge under plain version tags.
LEDGER = """
id = "9e0f1a2b-3c4d-4e5f-8a6b-7c8d9e0f1a2b"
slug = "ledger"
type = "library"

[package]
format = "crate"
crate = "ledger-rs"

[[releases]]
version = "0.9.0"
date = 2025-12-01
summary = "Published before the repository tagged its releases."

[[releases]]
version = "1.0.0"
date = 2026-01-02
summary = "First."

[[releases]]
version = "1.1.0"
date = 2026-02-03
summary = "Second, with `ledger::count` and *less* allocation."
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
        self.assertIn("lantern-Documentation/changelog/index.html", zipfile.ZipFile(self.root / "dist/lantern.zip").namelist())

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

    def test_a_release_keeps_the_identity_that_signed_it(self):
        mod_toml = self.root / "content/lantern/mod.toml"
        mod_toml.write_text(mod_toml.read_text().replace("[[releases]]", "[provenance]\nsigstore = true\n\n[[releases]]", 1))
        self.scratch.commit("Sign Lantern")
        signer = "https://github.com/DreamWeave-MP/StroggForge/.github/workflows/modGlobalBuild.yml@refs/tags/v{}"
        git(self.root, "tag", "lantern-1.0.0")
        git(self.root, "checkout", "-q", "lantern-1.0.0")
        build_site(self.root, "release", "lantern-1.0.0", env={"DREAMWEAVE_SIGNING_IDENTITY": signer.format(49)})
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        self.scratch.commit("RELEASE: Lantern 1.0.0")
        self.assertEqual(load(self.root, "content/lantern/mod.lock")["releases"][0]["signing_identity"], signer.format(49))

        build_site(self.root, "build", env={"DREAMWEAVE_SIGNING_IDENTITY": signer.format(50)})
        releases = {release["channel"]: release for release in self.manifest()["releases"]}
        self.assertEqual(releases["stable"]["artifacts"][0]["signatures"][0]["identity"], signer.format(49), "the pin moved after 1.0.0 was signed")
        self.assertEqual(releases["development"]["artifacts"][0]["signatures"][0]["identity"], signer.format(50))

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

    def stage_binaries(self, build: str, archives=BROOM_ARCHIVES) -> None:
        """What the workflow's download step leaves in dist/binaries/."""
        binaries = self.root / "dist/binaries"
        binaries.mkdir(parents=True, exist_ok=True)
        for archive in archives:
            (binaries / f"broom-{archive}").write_bytes(f"broom {archive} {build}".encode())

    def test_a_binary_release_records_every_platform_the_rust_workflow_built(self):
        self.add_broom()
        git(self.root, "tag", "1.0.0")
        git(self.root, "checkout", "-q", "1.0.0")
        process = build_site(self.root, "release", "1.0.0", check=False)
        self.assertIn("broom-Linux-X64.zip", process.stderr, "without the Rust workflow's archives there is nothing to release")
        self.stage_binaries("1.0.0")
        build_site(self.root, "release", "1.0.0")
        self.assertTrue((self.root / "dist/broom-Portmaster-ARM64.muxapp").is_file(), "the hashed bytes are the published bytes")
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        self.scratch.commit("RELEASE: Broom 1.0.0")

        locked = load(self.root, "content/broom/mod.lock")["releases"][0]
        self.assertEqual(
            [artifact["id"] for artifact in locked["artifacts"]],
            ["linux-x64", "windows-x64", "android-arm64", "linux-arm64-portmaster", "linux-arm64-muos"],
        )
        windows, muos = locked["artifacts"][1], locked["artifacts"][4]
        self.assertEqual(windows["format"], "binary")
        self.assertEqual(windows["platform"], {"os": "windows", "arch": "x86_64"})
        self.assertEqual(windows["digests"]["sha256"], hashlib.sha256(b"broom Windows-X64.zip 1.0.0").hexdigest())
        self.assertEqual(muos["filename"], "broom-Portmaster-ARM64.muxapp")
        self.assertEqual(muos["platform"], {"os": "linux", "arch": "aarch64", "variant": "muos"})
        self.assertEqual(locked["platforms"], [{"os": "linux", "arch": "x86_64"}, {"os": "windows", "arch": "x86_64"}], "a release's platform list is desktop systems only")

        self.stage_binaries("dev")
        build_site(self.root, "build")
        manifest = load(self.root, f"static/dreamweave/projects/{BROOM_ID}.json")
        self.assertEqual(manifest["channels"]["stable"], {"version": "1.0.0"})
        stable = next(release for release in manifest["releases"] if release["channel"] == "stable")
        self.assertEqual(stable["source"]["tag"], "1.0.0")
        self.assertEqual(stable["artifacts"][2]["sources"][0]["url"], "https://github.com/someone/cool-mods/releases/download/1.0.0/broom-Android-ARM64.zip")
        development = next(release for release in manifest["releases"] if release["channel"] == "development")
        self.assertEqual(development["artifacts"][0]["sources"][0]["url"], "https://github.com/someone/cool-mods/releases/download/development/broom-Linux-X64.zip")
        self.assertEqual(development["artifacts"][0]["digests"]["sha256"], hashlib.sha256(b"broom Linux-X64.zip dev").hexdigest())
        if jsonschema:
            self.assertEqual(schema_errors(manifest, "modManifest-2.schema.json"), [])

    def test_a_rust_projects_tags_are_bare_versions(self):
        self.add_broom()
        git(self.root, "tag", "broom-1.0.0")
        process = build_site(self.root, "release", "broom-1.0.0", check=False)
        self.assertIn("a Rust project's tags are bare versions: 1.0.0", process.stderr)

    def test_a_programs_page_offers_every_platform(self):
        self.add_broom()
        git(self.root, "tag", "1.0.0")
        self.stage_binaries("1.0.0")
        git(self.root, "checkout", "-q", "1.0.0")
        build_site(self.root, "release", "1.0.0")
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        self.scratch.commit("RELEASE: Broom 1.0.0")
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = html.unescape((self.root / "public/broom/index.html").read_text())
        hero = re.search(r'<div class="dw-actions dw-platforms".*?</div>', page, re.S).group(0)
        self.assertIn('data-platform="windows" href="https://github.com/someone/cool-mods/releases/download/1.0.0/broom-Windows-X64.zip"', hero)
        self.assertIn(">Linux <", hero)
        self.assertIn('data-platform="android"', hero)
        self.assertIn('data-platform="portmaster"', hero, "a handheld build is never marked as the visitor's desktop")
        self.assertIn(">muOS <", hero)
        self.assertNotIn("Mod manager", page, "a program is not handed to a mod manager")
        self.assertNotIn("OpenMW, by hand", page)
        self.assertIn("broom-&lt;platform&gt;.zip", (self.root / "public/broom/index.html").read_text())
        self.assertIn("<dt>Package</dt><dd>Program <small>5 platforms</small>", page)

    def add_ledger(self) -> None:
        self.scratch.add_project("ledger", LEDGER, title="Ledger", description="Counts things.")
        self.scratch.commit("Add Ledger")

    def fake_registry(self, crates: dict[str, bytes], corrupt: str | None = None) -> dict[str, str]:
        """A crates.io sparse index and download tree on disk, and the environment that points
        record-crates at them. `corrupt` names a version whose download does not match its index entry."""
        registry = tempfile.TemporaryDirectory(prefix="dreamweave-registry-")
        self.addCleanup(registry.cleanup)
        root = Path(registry.name)
        index = root / "index/le/dg"
        index.mkdir(parents=True)
        lines = [json.dumps({"name": "ledger-rs", "vers": version, "cksum": hashlib.sha256(data).hexdigest(), "deps": [], "features": {}, "yanked": False}) for version, data in crates.items()]
        (index / "ledger-rs").write_text("\n".join(lines) + "\n")
        downloads = root / "crates/ledger-rs"
        downloads.mkdir(parents=True)
        for version, data in crates.items():
            (downloads / f"ledger-rs-{version}.crate").write_bytes(data + (b" tampered" if version == corrupt else b""))
        return {"DREAMWEAVE_CRATES_INDEX": (root / "index").as_uri(), "DREAMWEAVE_CRATES_DOWNLOAD": (root / "crates").as_uri()}

    def test_crate_releases_are_recorded_from_crates_io(self):
        self.add_ledger()
        git(self.root, "tag", "1.0.0")
        self.assertIn("record-crates records it", build_site(self.root, "release", "1.0.0").stdout)
        self.assertFalse((self.root / "dist/release.json").exists(), "nothing is recorded at tag time")
        self.assertIn("a Rust project's tags are bare versions: 1.0.0", build_site(self.root, "release", "ledger-1.0.0", check=False).stderr)
        build_site(self.root, "build")
        self.assertEqual(load(self.root, "static/dreamweave/view.json")["projects"]["ledger/"]["unverified"], ["0.9.0", "1.0.0"], "0.9.0 has no tag, but 1.0.0 does: it was published before tagging began")
        self.assertEqual(load(self.root, f"static/dreamweave/projects/{LEDGER_ID}.json")["releases"], [])

        crates = {"0.9.0": b"ledger 0.9.0", "1.0.0": b"ledger 1.0.0"}
        output = build_site(self.root, "record-crates", env=self.fake_registry(crates)).stdout
        self.assertIn("ledger-rs 1.1.0 is not on crates.io yet", output)
        lock = load(self.root, "content/ledger/mod.lock")
        self.assertEqual([release["version"] for release in lock["releases"]], ["0.9.0", "1.0.0"])
        self.assertEqual(lock["releases"][1]["artifacts"], [{
            "id": "crate", "format": "crate", "filename": "ledger-rs-1.0.0.crate", "media_type": "application/gzip",
            "size": len(crates["1.0.0"]), "digests": {"sha256": hashlib.sha256(crates["1.0.0"]).hexdigest()},
        }])
        self.assertEqual(lock["releases"][1]["locked_from"], git(self.root, "rev-parse", "1.0.0"))
        self.assertEqual(lock["releases"][0]["locked_from"], "", "0.9.0 was never tagged")
        self.scratch.commit("RELEASE: Record Ledger's crates")

        build_site(self.root, "build")
        manifest = load(self.root, f"static/dreamweave/projects/{LEDGER_ID}.json")
        self.assertEqual(manifest["channels"], {"stable": {"version": "1.0.0"}})
        release = next(release for release in manifest["releases"] if release["version"] == "1.0.0")
        self.assertEqual(release["source"]["tag"], "1.0.0")
        self.assertEqual(release["artifacts"][0]["sources"], [{"url": "https://static.crates.io/crates/ledger-rs/ledger-rs-1.0.0.crate", "kind": "publisher"}])
        self.assertEqual(release["artifacts"][0]["signatures"], [])
        self.assertEqual(manifest["project"]["links"]["crate"], "https://crates.io/crates/ledger-rs")
        if jsonschema:
            self.assertEqual(schema_errors(manifest, "modManifest-2.schema.json"), [])

    def test_a_program_and_its_library_share_a_tag(self):
        self.add_broom()
        self.add_ledger()
        git(self.root, "tag", "1.0.0")
        git(self.root, "checkout", "-q", "1.0.0")
        self.stage_binaries("1.0.0")
        output = build_site(self.root, "release", "1.0.0").stdout
        self.assertIn("1.0.0 also releases ledger-rs 1.0.0", output)
        self.assertEqual(load(self.root, "dist/release.json")["project"], BROOM_ID, "the tag builds the program")
        git(self.root, "checkout", "-q", "main")
        build_site(self.root, "record")
        build_site(self.root, "record-crates", env=self.fake_registry({"0.9.0": b"ledger 0.9.0", "1.0.0": b"ledger 1.0.0"}))
        self.assertEqual([release["version"] for release in load(self.root, "content/broom/mod.lock")["releases"]], ["1.0.0"])
        self.assertEqual([release["version"] for release in load(self.root, "content/ledger/mod.lock")["releases"]], ["0.9.0", "1.0.0"])
        self.scratch.commit("RELEASE: Broom and Ledger 1.0.0")

        ledger = self.root / "content/ledger/mod.toml"
        ledger.write_text(ledger.read_text() + '\n[[releases]]\nversion = "1.2.0"\ndate = 2026-03-01\nsummary = "Faster."\n')
        self.scratch.commit("Declare Ledger 1.2.0")
        git(self.root, "tag", "1.2.0")
        self.assertIn("ledger-rs 1.2.0: StroggForge publishes it to crates.io", build_site(self.root, "release", "1.2.0").stdout, "a version only the library declares is the library's")

    def fake_github(self, releases: dict[str, dict[str, bytes]], digests: dict[str, str] | None = None) -> dict[str, str]:
        """GitHub's releases API and its assets on disk, and the environment that points
        record-releases at them. `releases` maps a tag to its assets; `digests` overrides the sha256
        GitHub reports for an asset."""
        github = tempfile.TemporaryDirectory(prefix="dreamweave-github-")
        self.addCleanup(github.cleanup)
        root = Path(github.name)
        tags = root / "api/repos/someone/cool-mods/releases/tags"
        tags.mkdir(parents=True)
        for tag, assets in releases.items():
            downloads = root / "download" / tag
            downloads.mkdir(parents=True)
            documents = []
            for name, data in assets.items():
                (downloads / name).write_bytes(data)
                digest = (digests or {}).get(name, hashlib.sha256(data).hexdigest())
                documents.append({"name": name, "size": len(data), "digest": f"sha256:{digest}", "browser_download_url": (downloads / name).as_uri()})
            (tags / tag).write_text(json.dumps({"tag_name": tag, "draft": False, "assets": documents}))
        return {"DREAMWEAVE_GITHUB_API": (root / "api").as_uri()}

    def add_broom_with_an_old_release(self) -> None:
        """Broom 0.9.0 was published before the repository was a site, under a v-prefixed tag."""
        self.add_broom()
        mod_toml = self.root / "content/broom/mod.toml"
        mod_toml.write_text(mod_toml.read_text().replace(
            '[[releases]]\nversion = "1.0.0"',
            '[[releases]]\nversion = "0.9.0"\ndate = 2025-12-01\ntag = "v0.9.0"\nsummary = "Before the site."\n\n[[releases]]\nversion = "1.0.0"',
        ))
        self.scratch.commit("Declare Broom 0.9.0")
        git(self.root, "tag", "v0.9.0")
        git(self.root, "tag", "1.0.0")

    def test_releases_published_before_the_site_are_recorded_from_github(self):
        self.add_broom_with_an_old_release()
        build_site(self.root, "build")
        self.assertEqual(load(self.root, "static/dreamweave/view.json")["projects"]["broom/"]["unverified"], ["0.9.0", "1.0.0"])

        old = {f"broom-{archive}": f"broom {archive} 0.9.0".encode() for archive in BROOM_ARCHIVES if not archive.endswith(".muxapp")}
        new = {f"broom-{archive}": f"broom {archive} 1.0.0".encode() for archive in BROOM_ARCHIVES}
        output = build_site(self.root, "record-releases", env=self.fake_github({"v0.9.0": old, "1.0.0": new})).stdout
        self.assertIn("Recorded Broom 0.9.0 from GitHub's release v0.9.0: 4 archive(s); it has no broom-Portmaster-ARM64.muxapp", output)
        lock = load(self.root, "content/broom/mod.lock")
        self.assertEqual([release["version"] for release in lock["releases"]], ["0.9.0", "1.0.0"])
        first = lock["releases"][0]
        self.assertEqual(first["locked_from"], git(self.root, "rev-parse", "v0.9.0"))
        self.assertEqual(first["artifacts"][0]["digests"]["sha256"], hashlib.sha256(b"broom Linux-X64.zip 0.9.0").hexdigest())
        self.assertEqual(len(lock["releases"][1]["artifacts"]), 5)
        self.scratch.commit("RELEASE: Record Broom's GitHub releases")
        self.assertNotIn("Recorded", build_site(self.root, "record-releases", env=self.fake_github({"v0.9.0": old, "1.0.0": new})).stdout, "a recorded release is not recorded again")

        build_site(self.root, "build")
        manifest = load(self.root, f"static/dreamweave/projects/{BROOM_ID}.json")
        self.assertEqual(manifest["channels"]["stable"], {"version": "1.0.0"})
        older = next(release for release in manifest["releases"] if release["version"] == "0.9.0")
        self.assertEqual(older["source"]["tag"], "v0.9.0")
        self.assertEqual(older["artifacts"][0]["sources"][0]["url"], "https://github.com/someone/cool-mods/releases/download/v0.9.0/broom-Linux-X64.zip")
        if jsonschema:
            self.assertEqual(schema_errors(manifest, "modManifest-2.schema.json"), [])
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = html.unescape((self.root / "public/broom/index.html").read_text())
        self.assertIn("https://github.com/someone/cool-mods/releases/download/v0.9.0/broom-Linux-X64.zip", page, "the page's model uses the release's own tag")

    def test_a_github_release_that_does_not_match_its_digest_is_not_recorded(self):
        self.add_broom_with_an_old_release()
        assets = {f"broom-{archive}": f"broom {archive} 1.0.0".encode() for archive in BROOM_ARCHIVES}
        process = build_site(self.root, "record-releases", env=self.fake_github({"1.0.0": assets}, digests={"broom-Windows-X64.zip": "0" * 64}), check=False)
        self.assertIn("but GitHub says " + "0" * 64, process.stderr)
        self.assertFalse((self.root / "content/broom/mod.lock").exists())

        output = build_site(self.root, "record-releases", env=self.fake_github({"1.0.0": assets})).stdout
        self.assertIn("v0.9.0 has no GitHub release, so Broom 0.9.0 stays unrecorded", output)
        self.assertEqual([release["version"] for release in load(self.root, "content/broom/mod.lock")["releases"]], ["1.0.0"])

    def test_a_release_under_its_own_tag_is_found_by_it(self):
        self.add_broom_with_an_old_release()
        process = build_site(self.root, "release", "v0.9.0", check=False)
        self.assertIn("v0.9.0: the Rust workflow's archives are missing from dist/binaries/: broom-Linux-X64.zip", process.stderr, "v0.9.0 names Broom's 0.9.0")

    def test_a_crate_that_does_not_match_its_index_entry_is_not_recorded(self):
        self.add_ledger()
        process = build_site(self.root, "record-crates", env=self.fake_registry({"1.0.0": b"ledger 1.0.0"}, corrupt="1.0.0"), check=False)
        self.assertIn("but the crates.io index says", process.stderr)
        self.assertFalse((self.root / "content/ledger/mod.lock").exists())

    def test_a_crates_page_says_how_to_add_it(self):
        self.add_ledger()
        git(self.root, "tag", "1.0.0")
        build_site(self.root, "record-crates", env=self.fake_registry({"0.9.0": b"ledger 0.9.0", "1.0.0": b"ledger 1.0.0"}))
        self.scratch.commit("RELEASE: Record Ledger's crates")
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = html.unescape((self.root / "public/ledger/index.html").read_text())
        self.assertIn('<div class="dw-command" aria-label="Add it with Cargo"><code>cargo add ledger-rs</code>', page)
        self.assertIn('ledger-rs = "1.0.0"', page)
        self.assertIn('href="https://crates.io/crates/ledger-rs/1.0.0"', page)
        self.assertIn("<dt>Package</dt><dd>Rust crate</dd>", page)
        self.assertNotIn("<span>Morrowind</span>", page, "a library that names no game is not labelled with one")
        self.assertIn('Pushing its tag, 1.1.0, publishes it">unreleased', page)
        self.assertIn('<p class="dw-release__summary">Second, with <code>ledger::count</code> and <em>less</em> allocation.</p>', page, "a release summary is Markdown")
        self.assertIn('<a href="#v1-0-0">1.0.0</a>', page)
        self.assertIn(f"Verify · sha256 {hashlib.sha256(b'ledger 1.0.0').hexdigest()[:12]}", page)
        self.assertNotIn("What is in the archive", page)
        self.assertNotIn("DreamWeave clients", page)
        self.assertNotIn("Mod manager", page)

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
        self.scratch.write("content/lantern/docs/_index.md", '+++\ntitle = "Lantern docs"\n+++\nThe manual.\n')
        self.scratch.write("content/lantern/docs/setup.md", '+++\ntitle = "Setup"\n+++\nSet it up.\n')
        self.scratch.write("content/lantern/materials/index.md", '+++\ntitle = "Materials"\n+++\nGrains.\n')
        self.scratch.write("content/lantern/.agents/helper.md", "Shipped as it is.\n")
        self.scratch.write("config.toml", 'ignored_content = ["**/.agents/**"]\n' + (self.root / "config.toml").read_text())
        self.scratch.commit("Add docs")
        build_site(self.root, "build")
        archive = zipfile.ZipFile(self.root / "dist/lantern.zip")
        names = set(archive.namelist())
        for expected in ("Lantern.omwscripts", "scripts/lantern/player.lua", "lantern-dwmod.toml", "dreamweave.release.json", "lantern-Documentation/index.html"):
            self.assertIn(expected, names)
        for rendered in ("index.md", "docs/_index.md", "docs/setup.md", "materials/index.md", "mod.toml", "Documentation/index.html"):
            self.assertNotIn(rendered, names, "the documentation ships rendered, not as its Markdown")
        self.assertIn(".agents/helper.md", names, "Markdown that is not a page is payload")
        self.assertEqual(archive.read("lantern-dwmod.toml"), (self.root / "content/lantern/mod.toml").read_bytes())
        self.assertNotIn("mod.lock", names)
        self.assertFalse(any(name.startswith("_changelog") for name in names))

        release = json.loads(archive.read("dreamweave.release.json"))
        self.assertEqual(release["project"]["id"], LANTERN_ID)
        if jsonschema:
            self.assertEqual(schema_errors(release, "dreamweave-release-payload-2.schema.json"), [])

        artifact = next(release for release in self.manifest()["releases"] if release["channel"] == "development")["artifacts"][0]
        self.assertEqual(artifact["layout"]["documentation"], "lantern-Documentation/index.html")
        page = archive.read("lantern-Documentation/index.html").decode()
        project_links = re.findall(r'(?:href|src)="https://example\.github\.io/cool-mods/lantern/[^"]*"', page)
        self.assertEqual(len(project_links), 1, f"only the offline banner's live-page link may stay absolute: {project_links}")
        self.assertIn('data-dw-online', page)
        self.assertIn('href="changelog/index.html"', page)
        for reference in re.findall(r'(?:href|src)="(_site/[^"#]+)"', page):
            self.assertIn(f"lantern-Documentation/{reference}", names, f"offline page references a file the archive lacks: {reference}")
        for info in archive.infolist():
            self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
            self.assertEqual(info.compress_type, zipfile.ZIP_STORED)

    def test_without_documentation_the_sources_ship(self):
        self.scratch.write("content/lantern/docs/_index.md", '+++\ntitle = "Lantern docs"\n+++\nThe manual.\n')
        mod_toml = self.root / "content/lantern/mod.toml"
        mod_toml.write_text(mod_toml.read_text() + "\n[package]\ndocumentation = false\n")
        self.scratch.commit("Ship the sources")
        build_site(self.root, "build")
        names = set(zipfile.ZipFile(self.root / "dist/lantern.zip").namelist())
        self.assertTrue({"index.md", "docs/_index.md", "lantern-dwmod.toml"} <= names, names)
        self.assertFalse(any(name.startswith("lantern-Documentation/") for name in names))

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
        self.assertIn("<span>Morrowind</span>", page, "game data names its game")
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
        # The content hash makes a changed icon, or another site's at the same address, a new URL.
        self.assertRegex(page, r'href="https://example\.github\.io/cool-mods/img/mark-teal\.svg\?h=[0-9a-f]+" type="image/svg\+xml"')
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

    def test_requirement_badges(self):
        index = self.root / "content/lantern/index.md"
        index.write_text(index.read_text() + '\n{{ requires(name="Tallow", url="https://example.com/tallow", note="Scheduling") }}\n{{ requires_openmw() }}\n')
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = (self.root / "public/lantern/index.html").read_text()
        self.assertIn('<a class="dw-requires" href="https://example.com/tallow"><span class="dw-requires__name">Tallow</span><span class="dw-requires__note">Scheduling</span></a>', page)
        self.assertIn('OpenMW 0.49+</span><span class="dw-requires__note">Required for use</span>', page, "the OpenMW badge follows [runtimes] in mod.toml")

    def test_author_text_is_escaped(self):
        hostile = "O'Brien's </script><script>alert(1)</script>"
        index = (self.root / "content/lantern/index.md").read_text().replace('title = "Lantern"', f"title = {json.dumps(hostile)}")
        (self.root / "content/lantern/index.md").write_text(index)
        mod_toml = (self.root / "content/lantern/mod.toml").read_text().replace('summary = "First."', 'summary = "<img src=x onerror=alert(2)> in `Vec<u8>`, see <https://example.com/>"\nhighlights = "<script>alert(3)</script>"\nfixed = ["<b onmouseover=alert(4)>bold</b>"]')
        (self.root / "content/lantern/mod.toml").write_text(mod_toml)
        self.scratch.commit("Hostile text")
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        page = (self.root / "public/lantern/index.html").read_text()
        self.assertNotIn("<script>alert(1)", page)
        self.assertNotIn("<img src=x onerror", page)
        self.assertNotIn("<script>alert(3)", page, "embedded HTML in release notes is shown as text")
        self.assertNotIn("<b onmouseover", page)
        self.assertIn("<code>Vec&lt;u8&gt;</code>", page, "a code span keeps its brackets")
        self.assertIn('href="https://example.com/">https://example.com/</a>', page, "an autolink stays a link")
        self.assertNotIn("&amp;#x27;", page, "text must be escaped once, not twice")
        self.assertIn("O&#x27;Brien", page)
        install_model = re.search(r'<script type="application/json" data-install-model>(.*?)</script>', page, re.S).group(1)
        self.assertNotIn("<", install_model)
        self.assertEqual(json.loads(install_model)["name"], hostile)


if __name__ == "__main__":
    unittest.main()
