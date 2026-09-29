"""./buildSite: the CI side of a DreamWeave mod site. Authors never run it: they edit, preview with
`zola serve`, commit, push, and push a tag to release. The workflow runs everything here."""

import argparse
import os
import sys
from pathlib import Path

from . import build, gitrepo, migrate, offline, sitecheck
from .problems import InvalidRepository

def command_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="buildSite",
        description="Validate, package and publish DreamWeave mod projects. Run from anywhere in the repository.",
        epilog="Don't forget to bring a towel.",
    )
    commands = parser.add_subparsers(dest="command", required=True)

    commands.add_parser("check", help="validate every project without writing anything")

    build_parser = commands.add_parser("build", help="package development builds and write the site's protocol files")
    build_parser.add_argument("--skip-archives", action="store_true", help="skip packaging (faster; development hashes are omitted)")

    release_parser = commands.add_parser("release", help="with a tag checked out: build that release into dist/, with its record for mod.lock")
    release_parser.add_argument("tag", help="<slug>-<version>, or a Rust project's <version>")

    record_parser = commands.add_parser("record", help="on the default branch: add the record `release` wrote to its project's mod.lock")
    record_parser.add_argument("--from", dest="source", default=str(build.RELEASE_RECORD), help=f"the record to add (default: {build.RELEASE_RECORD})")

    links_parser = commands.add_parser("links", help="check the built site's local links, assets and anchors")
    links_parser.add_argument("--public", default="public", help="the built site (default: public)")
    links_parser.add_argument("--base-url", help="the URL it was built for (default: DREAMWEAVE_BASE_URL or config.toml)")

    commands.add_parser("schemas", help="validate the generated index and manifests against the published schemas")
    commands.add_parser("record-crates", help="on the default branch: record the declared crate versions crates.io has in mod.lock")

    commands.add_parser("zola-version", help="print the Zola version archives are rendered with")
    return parser


def repository_root() -> Path:
    return Path(gitrepo.run_git("rev-parse", "--show-toplevel").decode().strip())


def run_build(root: Path, skip_archives: bool) -> None:
    repository = build.load_repository(root)
    repository.comments()
    repository.problems.raise_if_any()
    build.clean_dist(root)
    build.write_changelog_stubs(repository)
    artifacts = build.build_development(repository, include_archives=not skip_archives)
    build.write_site(repository, artifacts, archives_built=not skip_archives)
    print(f"Wrote {build.INDEX_FILE} and {len(repository.projects)} project manifest(s) under {build.GENERATED_ROOT}/projects/")


def main(arguments: list[str]) -> int:
    options = command_parser().parse_args(arguments)
    if options.command == "zola-version":
        print(offline.ZOLA_VERSION)
        return 0

    root = repository_root()
    os.chdir(root)
    try:
        if options.command == "check":
            repository = build.load_repository(root)
            repository.comments()
            for note in repository.problems.notes:
                print(f"note: {note}")
            for path in migrate.write_suggestions(root, repository.legacy_pages):
                print(f"Suggested {path.relative_to(root)}")
            repository.problems.raise_if_any()
            print(f"OK: {len(repository.projects)} project(s).")
        elif options.command == "build":
            run_build(root, options.skip_archives)
        elif options.command == "release":
            repository = build.load_repository(root)
            repository.problems.raise_if_any()
            build.clean_dist(root)
            build.build_release(repository, options.tag)
        elif options.command == "record":
            repository = build.load_repository(root, check_payloads=False)
            repository.problems.raise_if_any()
            build.record_release(repository, root / options.source)
        elif options.command == "links":
            import tomllib
            base_url = options.base_url or os.environ.get("DREAMWEAVE_BASE_URL") or tomllib.loads((root / "config.toml").read_text())["base_url"]
            checked, errors = sitecheck.check_site(root / options.public, base_url)
            if errors:
                print("\n".join(errors), file=sys.stderr)
                print(f"{len(errors)} broken local link(s).", file=sys.stderr)
                return 1
            print(f"Checked {checked} local links, assets and anchors.")
        elif options.command == "record-crates":
            repository = build.load_repository(root, check_payloads=False)
            repository.problems.raise_if_any()
            build.record_crate_releases(repository)
        elif options.command == "schemas":
            checked, errors = sitecheck.check_protocol_documents(root)
            if errors:
                print("\n".join(errors), file=sys.stderr)
                return 1
            print(f"{checked} protocol document(s) match their schemas.")
    except InvalidRepository as error:
        print(error.render(), file=sys.stderr)
        return 1
    except (gitrepo.GitError, offline.OfflineError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0
