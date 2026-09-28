"""Repository-level operations: load and cross-check projects, build archives, write site data."""

import hashlib
import json
import os
import shutil
import uuid
from dataclasses import dataclass, field
from pathlib import Path

from . import comments, fomod, gitrepo, offline, records
from .archive import ArchiveEntry, ArchiveResult, write_archive
from .model import (
    DEVELOPMENT_CHANNEL,
    EXAMPLE_PROJECT_IDS,
    MEDIA_IMAGE_SUFFIXES,
    MOD_LOCK,
    RUST_FORMATS,
    TEMPLATE_REPOSITORY,
    Project,
    SiteConfig,
    discover_project_directories,
    load_project,
    load_site_config,
    read_frontmatter,
)
from .payload import collect_payload
from .problems import Problems
from .versions import Version, VersionError

GENERATED_ROOT = Path("static") / "dreamweave"
VIEW_FILE = GENERATED_ROOT / "view.json"
INDEX_FILE = Path("static") / "dreamweave.json"
DIST = Path("dist")
RELEASE_RECORD = DIST / "release.json"
# The GitHub release the workflow publishes dist/ to: the tag, or the development release.
GITHUB_RELEASE = DIST / "github-release"
# Where the workflow puts the archives StroggForge's Rust workflow built for binary projects.
BINARIES = DIST / "binaries"


@dataclass
class Repository:
    root: Path
    site: SiteConfig
    projects: list[Project]
    locks: dict[str, list[records.LockedRelease]]
    head: str
    problems: Problems = field(default_factory=Problems)
    comments_setting: comments.CommentsSetting | None = None
    resolved_comments: dict | None = None
    legacy_pages: list[Path] = field(default_factory=list)

    def comments(self) -> dict:
        """The embed settings for comments, looked up once per run. See comments.py."""
        if self.resolved_comments is None:
            self.resolved_comments = comments.resolve_comments(self.site, self.comments_setting, self.problems)
        return self.resolved_comments

    def project_by_slug(self, slug: str) -> Project:
        for project in self.projects:
            if project.slug == slug:
                return project
        known = ", ".join(sorted(project.slug for project in self.projects)) or "none"
        raise SystemExit(f"No mod.toml project has slug {slug!r} (known: {known}).")

    def nested_directories(self, project: Project) -> list[str]:
        return [other.directory for other in self.projects if other.directory.startswith(f"{project.directory}/")]


def load_repository(root: Path, check_payloads: bool = True) -> Repository:
    problems = Problems()
    gitrepo.require_repository()
    site = load_site_config(root, problems)

    github_repository = os.environ.get("GITHUB_REPOSITORY")
    if github_repository and site.repository and github_repository.lower() != site.repository.lower():
        problems.error(
            "config.toml [extra]",
            f"github_username/github_project say {site.repository}, but this workflow is running in {github_repository}. "
            "Point them at the repository that publishes the site",
        )

    projects = []
    for directory in discover_project_directories(root):
        project = load_project(root, directory, problems)
        if project:
            records.check_openmw_runtime(project, problems)
            projects.append(project)

    seen_ids: dict[str, str] = {}
    seen_slugs: dict[str, str] = {}
    for project in projects:
        if project.id in seen_ids:
            problems.error(project.directory, f"id {project.id} is already used by {seen_ids[project.id]}")
        seen_ids[project.id] = project.directory
        if project.slug in seen_slugs:
            problems.error(project.directory, f"slug {project.slug!r} is already used by {seen_slugs[project.slug]}")
        seen_slugs[project.slug] = project.directory
        if project.id in EXAMPLE_PROJECT_IDS and site.repository.lower() != TEMPLATE_REPOSITORY.lower():
            problems.error(
                f"{project.directory}/mod.toml",
                f"id {project.id} belongs to the template's example project {EXAMPLE_PROJECT_IDS[project.id]}. "
                f"Every project needs its own identity: replace it with a fresh one, like {uuid.uuid4()}",
            )
        for included in project.package_include:
            if included.startswith("/") or ".." in included.split("/") or not (root / included).exists():
                problems.error(f"{project.directory}/mod.toml [package] include", f"{included!r} is not a file or directory in the repository; include paths start at its root")
        if (root / project.directory / "changelog.md").is_file():
            problems.error(
                f"{project.directory}/changelog.md",
                f"collides with the changelog page CI generates at /{project.page_path}changelog/. V4 wrote this file "
                "from commit messages; the changelog now comes from [[releases]] in mod.toml. Delete it",
            )
        for item in project.media:
            if item.file:
                check_media_file(root, project, item.file, problems)
            if item.thumbnail:
                check_media_file(root, project, item.thumbnail, problems)

    crates = [project.directory for project in projects if project.package_format == "crate"]
    if len(crates) > 1:
        problems.error(crates[1], f"a repository publishes one crate, tagged with plain versions; {crates[0]} already is it")

    legacy_pages = check_legacy_pages(root, projects, problems)
    head = gitrepo.resolve_revision("HEAD")
    locks = {project.id: records.read_lock(project, root, problems) for project in projects}
    for project in projects:
        check_release_order(project, locks[project.id], problems)

    comments_setting = comments.read_comments_setting(site, problems)
    repository = Repository(
        root=root, site=site, projects=projects, locks=locks, head=head, problems=problems,
        comments_setting=comments_setting, legacy_pages=legacy_pages,
    )
    if check_payloads:
        for project in projects:
            collect_payload(project, None, repository.nested_directories(project), problems, root)
    return repository


def check_release_order(project: Project, locked: list[records.LockedRelease], problems: Problems) -> None:
    """Within a channel, a later release must sort higher, or clients pick the wrong update.

    Only releases that are or can still be published count: tags pushed before this template
    recorded releases are history that can never enter the manifest, so their order is not a
    client's problem.
    """
    locked_versions = {release.version for release in locked}
    publishable = [
        release for release in project.releases
        if release.version in locked_versions or gitrepo.tag_revision(project.release_tag(release.version)) is None
    ]
    by_channel: dict[str, list] = {}
    for release in publishable:
        by_channel.setdefault(release.channel, []).append(release)
    for channel, channel_releases in by_channel.items():
        ordered = sorted(channel_releases, key=lambda release: (release.date, release.version.precedence_key()))
        for earlier, later in zip(ordered, ordered[1:]):
            if later.version < earlier.version:
                hint = (
                    'If this project numbers releases like decimals (0.82 then 0.9), set versioning = "decimal"; otherwise pick a version that sorts after the last one'
                    if project.versioning == "numeric" else "Pick a version that sorts after the last one"
                )
                problems.error(
                    f"{project.directory}/mod.toml",
                    f"{channel} release {later.version} ({later.date}) sorts below {earlier.version} ({earlier.date}) under {project.versioning} versioning. {hint}",
                )


def check_legacy_pages(root: Path, projects: list[Project], problems: Problems) -> list[Path]:
    """The V4 template, like V3 before it, kept project metadata in the frontmatter of
    content/<project>/index.md. V5 does not read it, so it must not linger. V4 only ever treated
    direct children of content/ as projects.
    Returns their directories, for which `check` writes suggested mod.toml files."""
    project_directories = {root / project.directory for project in projects}
    stale_directories = []
    for index in sorted((root / "content").glob("*/index.md")):
        if index.parent in project_directories:
            continue
        try:
            extra = (read_frontmatter(index) or {}).get("extra") or {}
        except Exception:
            continue
        stale = [key for key in ("version", "install_info", "nexus_id", "nexus_group_id", "offsite_host") if key in extra]
        if stale:
            stale_directories.append(index.parent)
            problems.error(
                index.relative_to(root).as_posix(),
                f"has V4 project frontmatter ({', '.join(stale)}) but no mod.toml. CI's check suggests one in the run's summary "
                "and its mod-toml-suggestions artifact; see content/guide/migration.md, or keep building this site from the template's V4 branch",
            )
    return stale_directories


def check_media_file(root: Path, project: Project, file: str, problems: Problems) -> None:
    where = f"{project.directory}/mod.toml media {file!r}"
    if file.startswith("/") or ".." in file.split("/") or "\\" in file:
        problems.error(where, "must be a path inside the project directory, like media/combat.webp")
        return
    if not file.lower().endswith(MEDIA_IMAGE_SUFFIXES):
        problems.error(where, f"is not an image ({', '.join(MEDIA_IMAGE_SUFFIXES)})")
    if not (root / project.directory / file).is_file():
        problems.error(where, "does not exist")


@dataclass
class ReleaseState:
    published: list[records.PublishedRelease]
    unverified: list[str]
    planned: list[str]
    on_registry: list[str]


def release_state(repository: Repository, project: Project) -> ReleaseState:
    """Published: recorded in mod.lock, which CI only does for a pushed tag. Unverified: tagged,
    but never recorded, like tags from before this template. Planned: declared, not tagged yet.
    On registry: a crate's tagged versions, which StroggForge published to crates.io; the
    manifest lists none of them, because crates.io, not this site, distributes a crate."""
    locked = {release.version: release for release in repository.locks[project.id]}
    published, unverified, planned, on_registry = [], [], [], []
    for declared in project.releases:
        tag = project.release_tag(declared.version)
        record = locked.get(declared.version)
        if project.package_format == "crate":
            (on_registry if gitrepo.tag_revision(tag) else planned).append(str(declared.version))
        elif record:
            published.append(records.PublishedRelease(declared=declared, locked=record, tag=tag, revision=record.locked_from or None, channel=declared.channel, date=declared.date))
        elif gitrepo.tag_revision(tag):
            unverified.append(str(declared.version))
        else:
            planned.append(str(declared.version))
    return ReleaseState(published=published, unverified=unverified, planned=planned, on_registry=on_registry)


def development_version(repository: Repository, project: Project, state: ReleaseState) -> Version:
    """The newest tagged release, published or not, plus the commits since its tag.

    Unrecorded tags still say what players may already have installed, so a development build
    must sort above them even though the manifest cannot list them.
    """
    tagged = [release.locked.version for release in state.published if gitrepo.tag_revision(release.tag)]
    tagged += [release.version for release in project.releases if str(release.version) in state.unverified]
    base = max(tagged, key=lambda version: version.precedence_key(), default=None)
    since = project.release_tag(base) if base is not None else None
    # CI's own record commits change only mod.lock, which no archive contains.
    count = gitrepo.count_commits(repository.head, since, project.directory, excluding=(f"{project.directory}/{MOD_LOCK}",))
    if base is None:
        base = Version.parse("0.0.0" if project.versioning == "numeric" else "0", project.versioning)
    return base.next_development(count)


def archive_entries(repository: Repository, project: Project, version: Version, revision: str | None, documentation: dict[str, bytes]) -> tuple[list[ArchiveEntry], dict]:
    problems = Problems()
    payload = collect_payload(project, revision, repository.nested_directories(project), problems, repository.root)
    problems.raise_if_any()

    contents = gitrepo.read_blobs([file.blob for file in payload if file.blob])
    entries = [
        ArchiveEntry(path=file.path, executable=file.executable, content=contents[file.blob] if file.blob else file.disk_path.read_bytes())
        for file in payload
    ]
    layout: dict = {"release_document": "dreamweave.release.json"}

    semantics = records.release_semantics(project)
    entries.append(ArchiveEntry("dreamweave.release.json", False, records.payload_release_document(project, version, semantics)))
    if project.package_documentation:
        entries.extend(ArchiveEntry(path, False, data) for path, data in sorted(documentation.items()))
        layout["documentation"] = f"{offline.DOCUMENTATION_ROOT}/index.html"
    if project.package_format == "fomod":
        website = f"{repository.site.base_url}/{project.page_path}"
        entries.append(ArchiveEntry("fomod/info.xml", False, fomod.info_xml(project, version, website)))
        entries.append(ArchiveEntry("fomod/ModuleConfig.xml", False, fomod.module_config_xml(project)))
        layout["installer"] = "fomod/ModuleConfig.xml"
    return entries, layout


def build_archive(repository: Repository, project: Project, version: Version, revision: str | None, documentation: dict[str, bytes]) -> tuple[ArchiveResult, dict]:
    entries, layout = archive_entries(repository, project, version, revision, documentation)
    filename = records.archive_filename(project)
    result = write_archive(repository.root / DIST / filename, entries)
    artifact = {
        "id": project.package_format,
        "format": project.package_format,
        "filename": filename,
        "media_type": records.MEDIA_TYPE_ZIP,
        "size": result.size,
        "digests": {"sha256": result.sha256},
        "layout": layout,
    }
    return result, artifact


def file_digest(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return path.stat().st_size, digest.hexdigest()


def collect_binaries(repository: Repository, project: Project) -> tuple[list[dict], list[str]]:
    """A binary project's archives, one per platform, exactly as the Rust workflow built them.

    The workflow downloads them into dist/binaries/; they are copied to dist/ unchanged, so the
    bytes published are the bytes hashed. Returns the artifacts and the archive names missing.
    """
    artifacts, missing = [], []
    for platform in project.platforms:
        filename = project.binary_archive(platform)
        source = repository.root / BINARIES / filename
        if not source.is_file():
            missing.append(filename)
            continue
        shutil.copyfile(source, repository.root / DIST / filename)
        size, sha256 = file_digest(source)
        artifacts.append({
            "id": platform.id,
            "format": "binary",
            "filename": filename,
            "media_type": records.MEDIA_TYPE_ZIP,
            "size": size,
            "digests": {"sha256": sha256},
            "platform": {"os": platform.system, "arch": platform.architecture},
        })
    return artifacts, missing


def stroggforge_inputs(repository: Repository) -> str:
    """What StroggForge's Rust workflows need, as GITHUB_OUTPUT lines. binary_names (a JSON list)
    and include_files (comma-separated, as it expects) build every binary project; crate_names (a
    JSON list) publishes the crate. dependents and benchmarks apply to both. Empty when there is
    nothing to build."""
    rust = [project for project in repository.projects if project.package_format in RUST_FORMATS]
    binaries = [project for project in rust if project.package_format == "binary"]
    names = sorted({project.package_binary for project in binaries})
    include = sorted({path for project in binaries for path in project.package_include})
    crates = sorted({project.package_crate for project in rust if project.package_format == "crate"})
    dependents = sorted({repository_name for project in rust for repository_name in project.build_dependents})
    benchmarks = any(project.build_benchmarks for project in rust)
    return "\n".join((
        f"binary_names={json.dumps(names)}",
        f"include_files={','.join(include)}",
        f"crate_names={json.dumps(crates)}",
        f"dependents={json.dumps(dependents)}",
        f"benchmarks={json.dumps(benchmarks)}",
    ))


def render_documentation(repository: Repository, projects: list[Project], packaged_versions: dict[str, Version]) -> dict[str, dict[str, bytes]]:
    documented = [project for project in projects if project.package_documentation]
    if not documented:
        return {}
    offline.require_zola(pinned=False)
    write_changelog_stubs(repository)
    write_view(repository, offline_mode=True, packaged_versions=packaged_versions)
    return offline.build_documentation(repository.root, repository.site.base_url, [project.page_path for project in documented])


def parse_release_tag(repository: Repository, tag: str) -> tuple[Project, Version]:
    """<slug>-<version>. Slugs cannot contain '-', so the first one separates them. A crate's tags
    are plain versions, and a repository has at most one crate."""
    crate = next((project for project in repository.projects if project.package_format == "crate"), None)
    if crate and tag[:1].isdigit():
        try:
            return crate, Version.parse(tag, crate.versioning)
        except VersionError as error:
            raise SystemExit(f"Tag {tag!r}: {error}") from error
    slug, separator, version_text = tag.partition("-")
    project = next((project for project in repository.projects if project.slug == slug), None) if separator else None
    if project is None:
        known = ", ".join("<version>" if project.package_format == "crate" else f"{project.slug}-<version>" for project in sorted(repository.projects, key=lambda project: project.slug)) or "none"
        raise SystemExit(f"Tag {tag!r} does not name a project. Release tags are <slug>-<version>; this commit's projects take {known}.")
    if project.package_format == "crate":
        raise SystemExit(f"Tag {tag!r}: a crate's tags are plain versions: {version_text}. StroggForge publishes those to crates.io.")
    try:
        return project, Version.parse(version_text, project.versioning)
    except VersionError as error:
        raise SystemExit(f"Tag {tag!r}: {error}") from error


def build_release(repository: Repository, tag: str) -> Path | None:
    """CI, on a tag: build the tagged release, and write dist/release.json, its record for mod.lock.

    Runs with the tag checked out, so the archive holds exactly what the tag does. Recording it on
    the default branch is record_release, run from a checkout of that branch.
    """
    project, version = parse_release_tag(repository, tag)
    if project.package_format == "crate":
        print(f"{tag} is {project.package_crate} {version}: StroggForge publishes it to crates.io, and there is nothing to record here.")
        return None
    revision = gitrepo.resolve_revision(f"refs/tags/{tag}")
    if revision != repository.head:
        raise SystemExit(f"Check out {tag} before building it; HEAD is {repository.head[:12]}, the tag is {revision[:12]}.")
    if project.declared_release(version) is None:
        raise SystemExit(
            f"{tag}: {project.directory}/mod.toml has no [[releases]] entry for {version} at this commit. Declare it with its "
            f"date and notes, commit, and move the tag there: git tag -f {tag} && git push -f origin {tag}"
        )
    if project.package_format == "binary":
        artifacts, missing = collect_binaries(repository, project)
        if missing:
            raise SystemExit(
                f"{tag}: the Rust workflow's archives are missing from {BINARIES}/: {', '.join(missing)}. A binary project's "
                "release is what StroggForge built for each of its [[platforms]]."
            )
    else:
        if project.package_documentation:
            offline.require_zola(pinned=True)
        documentation = render_documentation(repository, [project], {project.id: version})
        result, artifact = build_archive(repository, project, version, revision, documentation.get(project.page_path, {}))
        artifacts = [artifact]
    locked = records.LockedRelease(version=version, locked_from=revision, artifacts=artifacts, semantics=records.release_semantics(project))
    path = repository.root / RELEASE_RECORD
    path.write_text(records.dumps({"project": project.id, "name": project.name, "tag": tag, "release": locked.to_document()}), encoding="utf-8")
    for artifact in artifacts:
        print(f"Built {tag}: {artifact['filename']} {artifact['size']} bytes sha256 {artifact['digests']['sha256']}")
    write_nexus_uploads(repository, project, version, artifacts)
    write_release_notes(repository, project, version, artifacts)
    write_signing_list(repository, [(project, artifact) for artifact in artifacts])
    (repository.root / GITHUB_RELEASE).write_text(tag + "\n", encoding="utf-8")
    return path


def record_release(repository: Repository, release_path: Path) -> Path | None:
    """CI, on the default branch: add a release that build_release made to its project's mod.lock.

    Returns the lock's path if it changed. A release that is already recorded must match its record:
    a published archive never changes, whatever its tag points at now.
    """
    document = json.loads(release_path.read_text(encoding="utf-8"))
    tag = document["tag"]
    project = next((project for project in repository.projects if project.id == document["project"]), None)
    if project is None:
        raise SystemExit(
            f"{tag}: project {document['project']} ({document['name']}) is not on this branch, so it has no mod.lock to record "
            "the release in. Merge the tagged commit into this branch, then re-run the job."
        )
    problems = Problems()
    built = records.parse_locked_release(document["release"], project, str(release_path), problems)
    problems.raise_if_any()
    if project.declared_release(built.version) is None:
        raise SystemExit(
            f"{tag}: {project.directory}/mod.toml on this branch does not declare {built.version}. Merge the tagged commit "
            "into this branch, then re-run the job."
        )

    recorded = next((release for release in repository.locks[project.id] if release.version == built.version), None)
    if recorded is None:
        path = records.write_lock(project, repository.root, [*repository.locks[project.id], built])
        print(f"Recorded {tag} in {path.relative_to(repository.root)}")
        return path

    def fingerprint(release: records.LockedRelease) -> list[tuple]:
        return [(artifact["id"], artifact["size"], artifact["digests"]["sha256"]) for artifact in release.artifacts]

    if fingerprint(recorded) != fingerprint(built) or recorded.semantics != built.semantics:
        lines = [f"{tag} is not the release already recorded for {built.version}."]
        for label, release in (("mod.lock", recorded), ("this tag", built)):
            for artifact in release.artifacts:
                lines.append(f"  {label}: {artifact['filename']} {artifact['size']} bytes sha256 {artifact['digests']['sha256']}")
        if fingerprint(recorded) == fingerprint(built):
            lines.append("  The archives match, but the install or compatibility data in mod.toml does not.")
        lines.append(
            "A published release never changes: players and mirrors already have its bytes. Declare the next version in "
            "[[releases]] and tag that. If the tag moved by mistake, move it back to the commit mod.lock names "
            f"(locked_from {recorded.locked_from[:12]})."
        )
        raise SystemExit("\n".join(lines))
    print(f"{tag} matches its record in {project.directory}/mod.lock")
    return None


def build_development(repository: Repository, include_archives: bool) -> dict[str, list[dict]]:
    """Package every project's development build. Returns its artifacts keyed by project id.

    A binary project's development build is what the Rust workflow built from this commit. Without
    it (a pull request, or a build that failed) the project has no development channel this time.
    """
    targets = [project for project in repository.projects if project.package_development]
    versions = {project.id: development_version(repository, project, release_state(repository, project)) for project in targets}
    artifacts: dict[str, list[dict]] = {}
    (repository.root / GITHUB_RELEASE).parent.mkdir(parents=True, exist_ok=True)
    (repository.root / GITHUB_RELEASE).write_text(repository.site.development_release + "\n", encoding="utf-8")
    if not include_archives or not targets:
        return artifacts

    built = []
    for project in [project for project in targets if project.package_format == "binary"]:
        collected, missing = collect_binaries(repository, project)
        if missing:
            print(f"note: {project.slug} has no development build: {', '.join(missing)} not in {BINARIES}/")
            continue
        artifacts[project.id] = collected
        built.extend((project, artifact) for artifact in collected)
        print(f"Collected {project.slug} {versions[project.id]}: {len(collected)} platform archive(s)")

    packaged = [project for project in targets if project.package_format != "binary"]
    documentation = render_documentation(repository, packaged, versions) if packaged else {}
    for project in packaged:
        result, artifact = build_archive(repository, project, versions[project.id], None, documentation.get(project.page_path, {}))
        artifacts[project.id] = [artifact]
        built.append((project, artifact))
        print(f"Built {project.slug} {versions[project.id]}: {result.size} bytes sha256 {result.sha256}")
    write_signing_list(repository, built)
    return artifacts


def write_release_notes(repository: Repository, project: Project, version: Version, artifacts: list[dict]) -> Path:
    """dist/release-notes.md: the GitHub Release body, from the same notes as the changelog."""
    declared = project.declared_release(version)
    notes = records.notes_document(declared)
    lines = [f"## {project.name} {version}", ""]
    if "summary" in notes:
        lines += [notes["summary"], ""]
    if "highlights" in notes:
        lines += [notes["highlights"], ""]
    for key, heading in (("breaking", "Breaking changes"), ("added", "Added"), ("changed", "Changed"), ("fixed", "Fixed"), ("known_issues", "Known issues")):
        if key in notes:
            lines += [f"### {heading}", "", *(f"- {line}" for line in notes[key]), ""]
    if "migration" in notes:
        lines += ["### Migration", "", notes["migration"], ""]
    if "notes" in notes:
        lines += [notes["notes"], ""]
    base_url = site_base_url(repository)
    lines += [
        "---",
        "",
        *(f"`{artifact['filename']}` · {artifact['size']} bytes · SHA-256 `{artifact['digests']['sha256']}`  " for artifact in artifacts),
        "",
        f"Project page: {base_url}/{project.page_path} · Manifest: {base_url}/dreamweave/projects/{project.id}.json",
    ]
    path = repository.root / DIST / "release-notes.md"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def write_signing_list(repository: Repository, built: list[tuple[Project, dict]]) -> Path:
    """dist/sign.txt: archives whose projects asked for Sigstore signatures, one file name per line."""
    names = [artifact["filename"] for project, artifact in built if project.sigstore]
    path = repository.root / DIST / "sign.txt"
    path.write_text("".join(f"{name}\n" for name in names), encoding="utf-8")
    return path


def write_nexus_uploads(repository: Repository, project: Project, version: Version, artifacts: list[dict]) -> None:
    """dist/nexus.json: the Nexus Mods upload matrix for the workflow. Empty when nothing is configured."""
    uploads = []
    if project.nexusmods_file_group_id is not None:
        uploads.append({
            "name": project.name,
            "version": str(version),
            "file_group_id": project.nexusmods_file_group_id,
            "filename": artifacts[0]["filename"],
        })
    path = repository.root / DIST / "nexus.json"
    path.write_text(json.dumps(uploads), encoding="utf-8")


def write_changelog_stubs(repository: Repository) -> None:
    for project in repository.projects:
        path = repository.root / project.directory / "_changelog.md"
        path.write_text(
            f'+++\ntitle = {json.dumps(project.name + " changelog")}\nslug = "changelog"\ntemplate = "mod/changelog.html"\n\n[extra]\nproject = {json.dumps(project.page_path)}\ncomments = false\n+++\n',
            encoding="utf-8",
        )


def site_base_url(repository: Repository) -> str:
    return (os.environ.get("DREAMWEAVE_BASE_URL") or repository.site.base_url).rstrip("/")


def registry_date(project: Project, state: ReleaseState) -> str | None:
    """A crate's newest release date: the manifest lists none of its releases to take one from."""
    dates = [release.date for release in project.releases if str(release.version) in state.on_registry and release.date]
    return max(dates, default=None)


def write_site(repository: Repository, development_artifacts: dict[str, list[dict]], archives_built: bool) -> None:
    """Write the public protocol documents and the data the templates render from."""
    root = repository.root
    base_url = site_base_url(repository)
    generated = root / GENERATED_ROOT
    projects_directory = generated / "projects"
    projects_directory.mkdir(parents=True, exist_ok=True)
    for stale in projects_directory.glob("*.json"):
        stale.unlink()

    index_entries = []
    for project in repository.projects:
        state = release_state(repository, project)
        releases = [
            records.release_document(project, repository.site, release, release.tag, f"refs/tags/{release.tag}")
            for release in state.published
        ]
        if project.package_development and project.id in development_artifacts:
            version = development_version(repository, project, state)
            locked = records.LockedRelease(version=version, locked_from=repository.head, artifacts=development_artifacts[project.id], semantics=records.release_semantics(project))
            release_name = repository.site.development_release
            development = records.PublishedRelease(declared=None, locked=locked, tag=release_name, revision=repository.head, channel=DEVELOPMENT_CHANNEL, date=gitrepo.commit_time(repository.head)[:10])
            releases.append(records.release_document(project, repository.site, development, release_name, os.environ.get("DREAMWEAVE_DEVELOPMENT_REF", "refs/heads/main")))

        manifest = records.project_manifest(project, repository.site, base_url, releases)
        manifest_text = records.dumps(manifest)
        (projects_directory / f"{project.id}.json").write_text(manifest_text, encoding="utf-8")
        index_entries.append({
            "id": project.id,
            "name": project.name,
            **({"summary": project.summary} if project.summary else {}),
            "type": project.type,
            "status": project.status,
            "page": f"{base_url}/{project.page_path}",
            "manifest": f"{base_url}/dreamweave/projects/{project.id}.json",
            "manifest_sha256": hashlib.sha256(manifest_text.encode("utf-8")).hexdigest(),
            "updated": max((release["date"] for release in releases if "date" in release), default=registry_date(project, state)),
            "channels": {channel: head["version"] for channel, head in manifest["channels"].items()},
        })

    (root / INDEX_FILE).write_text(records.dumps(records.site_index(repository.site, base_url, index_entries)), encoding="utf-8")
    write_view(repository, offline_mode=False, archives_built=archives_built)


def write_view(repository: Repository, offline_mode: bool, packaged_versions: dict[str, Version] | None = None, archives_built: bool = True) -> None:
    from .view import build_view

    root = repository.root
    base_url = site_base_url(repository)
    view = build_view(repository, base_url, offline_mode, packaged_versions or {}, archives_built)
    path = root / VIEW_FILE
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(records.dumps(view), encoding="utf-8")


def clean_dist(root: Path) -> None:
    dist = root / DIST
    if dist.exists():
        for item in dist.iterdir():
            if item.is_file():
                item.unlink()
