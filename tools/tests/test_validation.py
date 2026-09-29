"""mod.toml and repository validation: every rule that protects the protocol, and its message."""

import os
import re
import unittest
from pathlib import Path

from support import LANTERN, LANTERN_FILES, Scratch, build_site, git
from dreamweave.build import load_repository
from dreamweave.model import load_project
from dreamweave.problems import Problems

MINIMAL = """
id = "0b8f1c2d-3e4a-4b5c-8d6e-7f8091a2b3c4"
slug = "lantern"
"""


class ProjectRules(unittest.TestCase):
    def setUp(self):
        self.scratch = Scratch()

    def tearDown(self):
        self.scratch.cleanup()

    def errors_for(self, mod_toml: str, frontmatter: str = 'title = "Lantern"') -> list[str]:
        self.scratch.write("content/lantern/index.md", f"+++\n{frontmatter}\n+++\nBody.\n")
        self.scratch.write("content/lantern/mod.toml", mod_toml)
        problems = Problems()
        load_project(self.scratch.root, self.scratch.root / "content/lantern", problems)
        return problems.errors

    def assertError(self, mod_toml: str, fragment: str, frontmatter: str = 'title = "Lantern"') -> None:
        errors = self.errors_for(mod_toml, frontmatter)
        self.assertTrue(any(fragment in error for error in errors), f"expected {fragment!r} in {errors}")

    def test_minimal_project_is_valid(self):
        self.assertEqual(self.errors_for(MINIMAL), [])

    def test_unknown_keys_are_errors_with_suggestions(self):
        self.assertError(MINIMAL + 'runtime = "openmw"\n', "did you mean 'runtimes'")
        self.assertError(MINIMAL + '[package]\nfromat = "bain"\n', "did you mean 'format'")

    def test_identity_must_be_a_canonical_uuid(self):
        self.assertError('id = "my-cool-mod"\nslug = "lantern"\n', "is not a UUID")
        self.assertError('id = "0B8F1C2D-3E4A-4B5C-8D6E-7F8091A2B3C4"\nslug = "lantern"\n', "canonical lowercase")
        self.assertError('id = "00000000-0000-0000-0000-000000000000"\nslug = "lantern"\n', "nil and max")

    def test_a_missing_or_copied_id_comes_with_a_fresh_one(self):
        # Authors never run the tooling, so the error itself hands them an identity to paste.
        errors = self.errors_for('slug = "lantern"\n')
        self.assertTrue(any(re.search(r'id = "[0-9a-f-]{36}"', error) for error in errors), errors)
        self.assertError('id = "my-cool-mod"\nslug = "lantern"\n', "Use a fresh random one, like")

    def test_slugs_cannot_contain_hyphens(self):
        self.assertError(MINIMAL.replace('"lantern"', '"cool-lantern"'), "release tags are <slug>-<version>")

    def test_display_name_comes_from_frontmatter(self):
        self.assertError(MINIMAL, "display name", frontmatter='description = "x"')

    def test_v4_frontmatter_beside_mod_toml_is_rejected(self):
        self.assertError(MINIMAL, "legacy release field", frontmatter='title = "Lantern"\n[extra]\nversion = "1.0"')

    def test_versions_must_be_unique_by_precedence(self):
        releases = '[[releases]]\nversion = "1.2"\ndate = 2026-01-01\n[[releases]]\nversion = "1.2.0"\ndate = 2026-02-01\n'
        self.assertError(MINIMAL + releases, "same precedence")

    def test_development_releases_are_not_declared(self):
        self.assertError(MINIMAL + '[[releases]]\nversion = "1.0"\nchannel = "development"\ndate = 2026-01-01\n', "built from your default branch")

    def test_release_dates_are_toml_dates(self):
        self.assertError(MINIMAL + '[[releases]]\nversion = "1.0"\ndate = "yesterday"\n', "TOML date")

    def test_yanked_and_deprecated_are_exclusive(self):
        self.assertError(MINIMAL + '[[releases]]\nversion = "1.0"\ndate = 2026-01-01\nyanked = "bad"\ndeprecated = "old"\n', "not both")

    def test_replacement_must_be_declared(self):
        self.assertError(MINIMAL + '[[releases]]\nversion = "1.0"\ndate = 2026-01-01\nyanked = "bad"\nreplacement = "1.1"\n', "not declared")

    def test_flat_packages_have_one_component_at_the_root(self):
        components = '[[components]]\nid = "core"\nname = "Core"\npath = "00 Core"\nrequired = true\n'
        self.assertError(MINIMAL + components, 'path "."')

    def test_bain_components_are_top_level_directories(self):
        components = '[package]\nformat = "bain"\n[[components]]\nid = "core"\nname = "Core"\npath = "00 Core/nested"\nrequired = true\n'
        self.assertError(MINIMAL + components, "top-level directories")

    def test_paths_cannot_escape(self):
        components = '[package]\nformat = "bain"\n[[components]]\nid = "core"\nname = "Core"\npath = "../outside"\nrequired = true\n'
        self.assertError(MINIMAL + components, "may not contain")

    def test_component_references_must_resolve(self):
        components = (
            '[package]\nformat = "bain"\n'
            '[[components]]\nid = "core"\nname = "Core"\npath = "00 Core"\nrequired = true\nrequires = ["ghost"]\n'
            '[[components]]\nid = "extra"\nname = "Extra"\npath = "10 Extra"\ngroup = "missing"\n'
        )
        errors = self.errors_for(MINIMAL + components)
        self.assertTrue(any("unknown component 'ghost'" in error for error in errors), errors)
        self.assertTrue(any("group 'missing' is not declared" in error for error in errors), errors)

    def test_group_selection_rules(self):
        components = (
            '[package]\nformat = "bain"\n'
            '[[groups]]\nid = "tex"\nname = "Textures"\nselect = "exactly-one"\n'
            '[[components]]\nid = "a"\nname = "A"\npath = "10 A"\ngroup = "tex"\ndefault = true\n'
            '[[components]]\nid = "b"\nname = "B"\npath = "11 B"\ngroup = "tex"\ndefault = true\n'
            '[[components]]\nid = "core"\nname = "Core"\npath = "00 Core"\nrequired = true\ngroup = "tex"\n'
        )
        errors = self.errors_for(MINIMAL + components)
        self.assertTrue(any("members default to installed" in error for error in errors), errors)
        self.assertTrue(any("cannot be one choice in a group" in error for error in errors), errors)

    def test_required_components_cannot_conflict(self):
        components = (
            '[package]\nformat = "bain"\n'
            '[[components]]\nid = "a"\nname = "A"\npath = "00 A"\nrequired = true\nconflicts = ["b"]\n'
            '[[components]]\nid = "b"\nname = "B"\npath = "01 B"\nrequired = true\n'
        )
        self.assertError(MINIMAL + components, "nothing can be installed")

    def test_relationship_rules(self):
        self.assertError(MINIMAL + '[[requires]]\nname = "Something"\nversion = ">=1"\n', "needs an `id`")
        self.assertError(MINIMAL + '[[requires]]\nid = "0b8f1c2d-3e4a-4b5c-8d6e-7f8091a2b3c4"\n', "cannot name the project itself")
        self.assertError(MINIMAL + '[[requires]]\nid = "11111111-2222-4333-8444-555555555555"\ncapability = "x"\n', "not both")
        self.assertError(MINIMAL + '[[requires]]\nversion = "=1"\n', "needs an `id` (a DreamWeave project), a `capability`")
        self.assertError(MINIMAL + '[[requires]]\nid = "11111111-2222-4333-8444-555555555555"\nversion = "2+"\n', "not a comparator")

    def test_extension_namespaces(self):
        self.assertEqual(self.errors_for(MINIMAL + '[extensions."org.tes3mp"]\nserver_side = true\n'), [])
        self.assertError(MINIMAL + '[extensions.tes3mp]\nx = 1\n', "must be dotted")
        self.assertError(MINIMAL + '[extensions.openmw]\nx = 1\n', "top-level [openmw] table")

    def test_mirror_templates(self):
        self.assertEqual(self.errors_for(MINIMAL + '[[mirrors]]\nurl = "https://cache.example.org/sha256/{sha256}"\n'), [])
        self.assertError(MINIMAL + '[[mirrors]]\nurl = "https://cache.example.org/{hash}"\n', "unknown placeholder")
        self.assertError(MINIMAL + '[[mirrors]]\nurl = "https://cache.example.org/{slug}/"\n', "every artifact would share one URL")

    def test_binary_packages(self):
        binary = MINIMAL + 'type = "tool"\n[package]\nformat = "binary"\nbinary = "broom"\n[[platforms]]\nos = "linux"\narch = "x86_64"\n'
        self.assertEqual(self.errors_for(binary), [])
        self.assertError(MINIMAL + '[package]\nformat = "binary"\n[[platforms]]\nos = "linux"\narch = "x86_64"\n', "names the Cargo binary")
        self.assertError(MINIMAL + '[package]\nformat = "binary"\nbinary = "broom"\n', "lists the [[platforms]] it is built for")
        self.assertError(binary.replace('[[platforms]]', '[[components]]\nid = "core"\nname = "Core"\npath = "00 Core"\n\n[[platforms]]'), "has no [[components]]")
        self.assertError(binary + '[openmw]\ncontent_files = ["Broom.omwscripts"]\n', "no [openmw] install data")
        self.assertError(binary.replace('binary = "broom"', 'binary = "broom"\ndocumentation = true'), "cannot add the rendered docs")
        self.assertError(MINIMAL + '[package]\nbinary = "broom"\n', 'only format = "binary" packages have a binary')
        self.assertError(MINIMAL + '[package]\ninclude = ["README.md"]\n', "only format = \"binary\" packages include extra files")
        self.assertError(binary + '[[platforms]]\nos = "linux"\narch = "x86_64"\n', "listed twice")

    def test_crate_packages(self):
        crate = MINIMAL + 'type = "library"\n[package]\nformat = "crate"\ncrate = "lantern-light"\n'
        self.assertEqual(self.errors_for(crate), [])
        self.assertError(MINIMAL + '[package]\nformat = "crate"\n', "names its crates.io package")
        self.assertError(MINIMAL + '[package]\ncrate = "lantern-light"\n', 'only format = "crate" and "binary" packages have a crate')
        self.assertError(crate.replace('crate = "lantern-light"', 'crate = "lantern light"'), "is not a crates.io package name")
        self.assertError(crate.replace('crate = "lantern-light"', 'crate = "lantern-light"\ndevelopment = true'), "has no development build")
        self.assertError(crate.replace('crate = "lantern-light"', 'crate = "lantern-light"\ndocumentation = true'), "the site is its documentation")
        self.assertError(crate + '[[components]]\nid = "core"\nname = "Core"\npath = "00 Core"\n', "it is one library")
        self.assertError(crate + '[openmw]\ncontent_files = ["Lantern.omwscripts"]\n', "not data OpenMW loads")
        self.assertError(crate + '[[mirrors]]\nurl = "https://cache.example.org/sha256/{sha256}"\n', "no [[mirrors]]")
        self.assertError(crate + '[[platforms]]\nos = "linux"\narch = "x86_64"\n', "no [[platforms]]")
        # StroggForge's inputs live in the repository's Rust workflow, not in mod.toml.
        self.assertError(crate + '[build]\nbenchmarks = true\n', "build")
        self.assertError(crate + '[provenance]\nsigstore = true\n', "sigstore signs mod archives")

    def test_platform_variants(self):
        program = MINIMAL + 'type = "tool"\n[package]\nformat = "binary"\nbinary = "broom"\ncrate = "broom"\n[[platforms]]\nos = "linux"\narch = "x86_64"\n'
        handheld = '[[platforms]]\nos = "android"\narch = "aarch64"\n\n[[platforms]]\nos = "linux"\narch = "aarch64"\nvariant = "portmaster"\n\n[[platforms]]\nos = "linux"\narch = "aarch64"\nvariant = "muos"\n'
        self.assertEqual(self.errors_for(program + handheld), [], "a program may name its crate and list handheld builds")
        self.assertError(program + '[[platforms]]\nos = "linux"\narch = "aarch64"\nvariant = "switch"\n', "'switch' is not one of portmaster, muos")
        self.assertError(program.replace('[[platforms]]\nos = "linux"\narch = "x86_64"\n', handheld), "at least one desktop platform")
        self.assertError(MINIMAL + '[[platforms]]\nos = "android"\narch = "aarch64"\n', "only format = \"binary\" packages list them")

    def test_media_needs_alt_text(self):
        self.assertError(MINIMAL + '[[media]]\nfile = "media/a.webp"\n', "alt")

    def test_openmw_file_names_are_not_paths(self):
        self.assertError(MINIMAL + '[openmw]\ncontent_files = ["sub/Lantern.esp"]\n', "is a file name, not a path")
        self.assertError(MINIMAL + '[openmw]\ncontent_files = ["Lantern.txt"]\n', "should end in")


class RepositoryRules(unittest.TestCase):
    def setUp(self):
        self.scratch = Scratch()

    def tearDown(self):
        self.scratch.cleanup()
        os.environ.pop("GITHUB_REPOSITORY", None)

    def errors(self) -> list[str]:
        self.scratch.commit()
        current = Path.cwd()
        os.chdir(self.scratch.root)
        try:
            return load_repository(self.scratch.root).problems.errors
        finally:
            os.chdir(current)

    def assertError(self, fragment: str) -> None:
        errors = self.errors()
        self.assertTrue(any(fragment in error for error in errors), f"expected {fragment!r} in {errors}")

    def test_a_valid_project(self):
        self.scratch.add_project("lantern", LANTERN, files=LANTERN_FILES)
        self.assertEqual(self.errors(), [])

    def test_declared_content_files_must_exist_with_their_exact_case(self):
        self.scratch.add_project("lantern", LANTERN, files={"lantern.omwscripts": "PLAYER: x.lua\n"})
        self.assertError("the file is 'lantern.omwscripts'; case matters")

    def test_missing_content_file(self):
        self.scratch.add_project("lantern", LANTERN, files={"scripts/x.lua": "return {}\n"})
        self.assertError("which is not in any of its data directories")

    def test_paths_differing_only_by_case(self):
        self.scratch.add_project("lantern", LANTERN, files={**LANTERN_FILES, "Textures/a.dds": "a", "textures/A.dds": "b"})
        self.assertError("differ only by case")

    def test_windows_reserved_names(self):
        self.scratch.add_project("lantern", LANTERN, files={**LANTERN_FILES, "aux.txt": "x", "notes./x.txt": "y"})
        errors = self.errors()
        self.assertTrue(any("reserves for devices" in error for error in errors), errors)
        self.assertTrue(any("ending in a space or dot" in error for error in errors), errors)

    def test_symlinks_are_refused(self):
        self.scratch.add_project("lantern", LANTERN, files=LANTERN_FILES)
        (self.scratch.root / "content/lantern/escape").symlink_to("/etc/passwd")
        self.assertError("is a symlink")

    def test_generated_archive_paths_cannot_be_shadowed(self):
        self.scratch.add_project("lantern", LANTERN, files={**LANTERN_FILES, "Documentation/readme.txt": "mine"})
        self.assertError("which DreamWeave generates inside the archive")

    def test_ids_and_slugs_are_unique_across_the_site(self):
        self.scratch.add_project("lantern", LANTERN, files=LANTERN_FILES)
        self.scratch.add_project("copy", LANTERN, title="Copy", files=LANTERN_FILES)
        errors = self.errors()
        self.assertTrue(any("id 0b8f1c2d-3e4a-4b5c-8d6e-7f8091a2b3c4 is already used by content/" in error for error in errors), errors)
        self.assertTrue(any("slug 'lantern' is already used by content/" in error for error in errors), errors)

    def test_a_repository_has_one_rust_project(self):
        crate = 'type = "library"\n[package]\nformat = "crate"\ncrate = "alpha"\n'
        program = 'type = "tool"\n[package]\nformat = "binary"\nbinary = "beta"\n[[platforms]]\nos = "linux"\narch = "x86_64"\n'
        self.scratch.add_project("alpha", 'id = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d"\nslug = "alpha"\n' + crate, title="Alpha")
        self.scratch.add_project("beta", 'id = "2b3c4d5e-6f7a-4b8c-9d0e-1f2a3b4c5d6e"\nslug = "beta"\n' + program, title="Beta")
        self.assertError("a repository has one Rust project, released under bare version tags; content/alpha already is it")

    def test_a_v4_changelog_file_is_refused(self):
        self.scratch.add_project("lantern", LANTERN, files={**LANTERN_FILES, "changelog.md": "+++\ntitle = \"Changelog\"\n+++\n"})
        self.assertError("collides with the changelog page CI generates at /lantern/changelog/")

    def test_example_ids_cannot_be_reused(self):
        self.scratch.add_project("lantern", LANTERN.replace("0b8f1c2d-3e4a-4b5c-8d6e-7f8091a2b3c4", "4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90"), files=LANTERN_FILES)
        self.assertError("belongs to the template's example project")

    def test_ci_repository_must_match_config(self):
        self.scratch.add_project("lantern", LANTERN, files=LANTERN_FILES)
        os.environ["GITHUB_REPOSITORY"] = "someone-else/their-mods"
        self.assertError("this workflow is running in someone-else/their-mods")

    def test_repository_names_follow_github_rules(self):
        config = (self.scratch.root / "config.toml").read_text().replace('github_project = "cool-mods"', 'github_project = "cool mods<script>"')
        (self.scratch.root / "config.toml").write_text(config)
        self.assertError("is not a GitHub repository name")

    def test_palette_must_exist(self):
        config = (self.scratch.root / "config.toml").read_text().replace("[extra]\n", '[extra]\npalette = "blue"\n', 1)
        (self.scratch.root / "config.toml").write_text(config)
        self.assertError("'blue' is not one of purple, teal, gold, ember, moss, umber")

    def test_v4_pages_without_mod_toml_are_rejected(self):
        self.scratch.write("content/old/index.md", '+++\ntitle = "Old"\n[extra]\nversion = "0.5"\n+++\n')
        self.assertError("has V4 project frontmatter")

    def test_a_scratch_check_never_writes_to_the_real_runs_summary(self):
        summary = self.scratch.root.parent / f"{self.scratch.root.name}-summary.md"
        summary.write_text("")
        os.environ["GITHUB_STEP_SUMMARY"] = str(summary)
        try:
            self.scratch.write("content/old/index.md", '+++\ntitle = "Old"\n[extra]\nversion = "0.5"\n+++\n')
            self.scratch.commit("V4 page")
            build_site(self.scratch.root, "check", check=False)
            self.assertEqual(summary.read_text(), "", "the scratch site's suggestions reached the summary of the run executing the tests")
        finally:
            os.environ.pop("GITHUB_STEP_SUMMARY", None)
            summary.unlink()

    def test_the_check_suggests_a_mod_toml_for_each_v4_page(self):
        self.scratch.write("content/old/index.md", '+++\ntitle = "Old Lamp"\n[extra]\nversion = "0.6"\n[extra.install_info]\ncontent_files = ["Old.omwscripts"]\n+++\nBody.\n')
        self.scratch.write("content/old/Old.omwscripts", "PLAYER: scripts/old.lua\n")
        self.scratch.commit("V4 page")
        git(self.scratch.root, "tag", "old_lamp-0.5")
        process = build_site(self.scratch.root, "check", check=False)
        self.assertNotEqual(process.returncode, 0)
        self.assertIn("mod-toml-suggestions", process.stderr)
        suggestion = (self.scratch.root / "dist/migration/content/old/mod.toml").read_text()
        self.assertTrue(suggestion.startswith("# Suggested from the V4 frontmatter"))
        self.assertIn("# - ", suggestion, "what the converter could not decide travels with the file")
        self.assertIn('slug = "old_lamp"', suggestion)

        (self.scratch.root / "content/old/mod.toml").write_text(suggestion)
        self.scratch.write("content/old/index.md", '+++\ntitle = "Old Lamp"\n+++\nBody.\n')
        self.scratch.commit("Adopt the suggestion")
        self.assertEqual(build_site(self.scratch.root, "check").returncode, 0)

    def test_a_newer_release_cannot_sort_below_an_older_one(self):
        releases = '[[releases]]\nversion = "0.82"\ndate = 2026-01-01\n[[releases]]\nversion = "0.9"\ndate = 2026-02-01\n'
        self.scratch.add_project("lantern", LANTERN.split("[[releases]]")[0] + releases, files=LANTERN_FILES)
        self.assertError('set versioning = "decimal"')

    def test_decimal_versioning_accepts_decimal_histories(self):
        releases = '[[releases]]\nversion = "0.82"\ndate = 2026-01-01\n[[releases]]\nversion = "0.9"\ndate = 2026-02-01\n'
        self.scratch.add_project("lantern", 'versioning = "decimal"\n' + LANTERN.split("[[releases]]")[0] + releases, files=LANTERN_FILES)
        self.assertEqual(self.errors(), [])

    def test_decimal_versioning_still_orders(self):
        releases = '[[releases]]\nversion = "0.82"\ndate = 2026-01-01\n[[releases]]\nversion = "0.8"\ndate = 2026-02-01\n'
        self.scratch.add_project("lantern", 'versioning = "decimal"\n' + LANTERN.split("[[releases]]")[0] + releases, files=LANTERN_FILES)
        self.assertError("sorts below 0.82 (2026-01-01) under decimal")

    def test_tags_pushed_before_any_lock_are_history_not_errors(self):
        releases = '[[releases]]\nversion = "0.5"\ndate = 2024-01-01\n[[releases]]\nversion = "0.1"\ndate = 2024-05-01\n'
        self.scratch.add_project("lantern", LANTERN.split("[[releases]]")[0] + releases, files=LANTERN_FILES)
        self.scratch.commit()
        git(self.scratch.root, "tag", "lantern-0.5")
        git(self.scratch.root, "tag", "lantern-0.1")
        self.assertEqual(self.errors(), [])

    def test_openmw_install_data_needs_an_openmw_runtime(self):
        self.scratch.add_project("lantern", LANTERN.replace('[runtimes]\nopenmw = ">=0.49"\n', ""), files=LANTERN_FILES)
        self.assertError("no OpenMW runtime")


if __name__ == "__main__":
    unittest.main()
