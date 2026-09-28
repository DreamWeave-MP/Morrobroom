"""Comments: the build looks up giscus ids for the site's own repository; no comment text is built in."""

import json
import shutil
import subprocess
import unittest
import urllib.error

from support import LANTERN, LANTERN_FILES, Scratch, build_site
from dreamweave.comments import read_comments_setting, resolve_comments
from dreamweave.model import SiteConfig
from dreamweave.problems import Problems

TEMPLATE_ANSWER = {
    "repositoryId": "R_kgDOQtd5bg",
    "categories": [{"id": "DIC_kwDOQtd5bs4C0JZa", "name": "General"}, {"id": "DIC_kwDOQtd5bs4C0JZb", "name": "Q&A"}],
}


def site(extra: dict) -> SiteConfig:
    return SiteConfig(title="Cool Mods", base_url="https://example.github.io/cool-mods", repository_owner="someone", repository_name="cool-mods", extra=extra)


class Lookup(unittest.TestCase):
    def resolve(self, extra: dict, fetch):
        problems = Problems()
        configuration = site(extra)
        result = resolve_comments(configuration, read_comments_setting(configuration, problems), problems, fetch=fetch)
        return result, problems

    def test_ids_come_from_the_sites_own_repository(self):
        asked = []
        result, problems = self.resolve({"comments": {"category": "Q&A"}}, lambda repository: asked.append(repository) or TEMPLATE_ANSWER)
        self.assertEqual(asked, ["someone/cool-mods"])
        self.assertEqual(problems.errors, [])
        self.assertEqual((result["state"], result["repo"], result["repo_id"], result["category_id"]), ("on", "someone/cool-mods", "R_kgDOQtd5bg", "DIC_kwDOQtd5bs4C0JZb"))

    def test_no_comments_table_means_no_lookup(self):
        result, _ = self.resolve({}, lambda repository: self.fail("looked up comments nobody asked for"))
        self.assertEqual(result, {"state": "off"})

    def test_disabled_means_no_lookup(self):
        result, _ = self.resolve({"comments": {"enabled": False}}, lambda repository: self.fail("looked up disabled comments"))
        self.assertEqual(result, {"state": "off"})

    def test_giscus_not_installed_is_a_warning_not_a_failure(self):
        result, problems = self.resolve({"comments": {}}, lambda repository: {"error": "giscus is not installed on this repository"})
        self.assertEqual(result["state"], "not-installed")
        self.assertIn("install https://github.com/apps/giscus", result["message"])
        self.assertEqual(problems.errors, [])

    def test_unreachable_giscus_is_a_warning_not_a_failure(self):
        def offline(repository):
            raise urllib.error.URLError("no route to host")
        result, problems = self.resolve({"comments": {}}, offline)
        self.assertEqual(result["state"], "unreachable")
        self.assertEqual(problems.errors, [])

    def test_a_category_that_does_not_exist_is_an_error(self):
        result, problems = self.resolve({"comments": {"category": "Comments"}}, lambda repository: TEMPLATE_ANSWER)
        self.assertEqual(result["state"], "off")
        self.assertTrue(any("'Comments' is not a Discussions category" in error and "General, Q&A" in error for error in problems.errors), problems.errors)

    def test_v4_pasted_ids_are_refused(self):
        _, problems = self.resolve({"giscus": {"repo_id": "R_kgDOQtd5bg", "category_id": "DIC_kwDOQtd5bs4C0JZa"}}, lambda repository: TEMPLATE_ANSWER)
        self.assertTrue(any("V4 comments setting" in error for error in problems.errors), problems.errors)

    def test_unknown_keys_are_errors(self):
        _, problems = self.resolve({"comments": {"repo_id": "R_x"}}, lambda repository: TEMPLATE_ANSWER)
        self.assertTrue(any("repo_id" in error and "not a recognized key" in error for error in problems.errors), problems.errors)


@unittest.skipUnless(shutil.which("zola"), "renders the site")
class Rendering(unittest.TestCase):
    def setUp(self):
        self.scratch = Scratch()
        self.root = self.scratch.root
        self.scratch.add_project("lantern", LANTERN, files=LANTERN_FILES)
        self.scratch.commit("Add Lantern")

    def tearDown(self):
        self.scratch.cleanup()

    def render_with_comments(self) -> None:
        build_site(self.root, "build")
        view_path = self.root / "static/dreamweave/view.json"
        view = json.loads(view_path.read_text())
        view["comments"] = {"state": "on", "repo": "someone/cool-mods", "repo_id": "R_test", "category": "General", "category_id": "DIC_test", "reactions": False, "theme": None, "mapping": "pathname"}
        view_path.write_text(json.dumps(view))
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)

    def test_project_pages_embed_giscus_with_v4_thread_matching(self):
        self.render_with_comments()
        page = (self.root / "public/lantern/index.html").read_text()
        for attribute in ('data-repo="someone/cool-mods"', 'data-repo-id="R_test"', 'data-category-id="DIC_test"', 'data-mapping="pathname"', 'data-strict="0"', 'data-loading="lazy"'):
            self.assertIn(attribute, page)
        self.assertIn('data-theme="https://example.github.io/cool-mods/giscus/purple.css"', page)
        self.assertIn('href="#comments"', page)
        comments, rail = page.index('id="comments"'), page.index('class="dw-project__rail"')
        self.assertLess(comments, rail, "the thread belongs in the body column, before the rail, not below the page")
        self.assertNotIn("dw-comments__preview", page, "an https site uses its own theme and needs no preview note")

    def test_changelogs_and_offline_documentation_have_no_comments(self):
        self.render_with_comments()
        self.assertNotIn("giscus.app/client.js", (self.root / "public/lantern/changelog/index.html").read_text())
        import zipfile
        archive = zipfile.ZipFile(self.root / "dist/lantern.zip")
        self.assertFalse(any("giscus" in archive.read(name).decode(errors="ignore") for name in archive.namelist() if name.endswith(".html")))

    def test_no_comments_without_a_lookup(self):
        build_site(self.root, "build")
        subprocess.run(["zola", "build"], cwd=self.root, check=True, capture_output=True)
        self.assertNotIn("giscus.app/client.js", (self.root / "public/lantern/index.html").read_text())


if __name__ == "__main__":
    unittest.main()
