"""Release records, the lock file, and the published JSON documents.

A release has two halves. Its install and compatibility semantics and its artifacts are frozen
in mod.lock when CI publishes it, because they describe bytes that already exist. Its date,
channel, notes and yank state stay in mod.toml, because those are statements about the release
that an author may need to correct later.
"""

import json
from dataclasses import dataclass
from pathlib import Path

from .model import DEVELOPMENT_CHANNEL, MOD_LOCK, DeclaredRelease, Project, SiteConfig
from .problems import Problems
from .versions import Version, VersionError

SCHEMA_VERSION = "2"
GENERATOR = "DreamWeave Mod Template 5.0.0"
MEDIA_TYPE_ZIP = "application/zip"
SIGSTORE_ISSUER = "https://token.actions.githubusercontent.com"
WORKFLOW_PATH = ".github/workflows/build_site.yml"
ARTIFACT_KEYS = ("id", "format", "filename", "media_type", "size", "digests")
SEMANTIC_KEYS = ("runtimes", "platforms", "provides", "relationships", "components", "groups", "extensions")


def dumps(document: object) -> str:
    return json.dumps(document, indent=2, ensure_ascii=False, sort_keys=False) + "\n"


def release_semantics(project: Project) -> dict:
    """The part of a release that clients act on. Frozen into mod.lock for published releases."""
    semantics: dict = {
        "runtimes": {runtime: str(constraint) for runtime, constraint in project.runtimes.items()},
        "platforms": [{"os": platform.system, "arch": platform.architecture} for platform in project.platforms],
        "provides": list(project.provides),
        "relationships": [relationship_document(relationship) for relationship in project.relationships],
        "components": [
            {
                "id": component.id,
                "name": component.name,
                **({"description": component.description} if component.description else {}),
                "path": component.path,
                "required": component.required,
                "default": component.default,
                **({"group": component.group} if component.group else {}),
                "requires": list(component.requires),
                "conflicts": list(component.conflicts),
                "suggested_with": list(component.suggested_with),
            }
            for component in project.components
        ],
        "groups": [
            {
                "id": group.id,
                "name": group.name,
                "select": group.selection,
                **({"description": group.description} if group.description else {}),
            }
            for group in project.groups
        ],
        "extensions": {},
    }

    if "openmw" in project.runtimes:
        openmw: dict = {
            "components": {
                component.id: {
                    "data_directories": list(component.openmw.data_directories),
                    "content_files": list(component.openmw.content_files),
                    "groundcover_files": list(component.openmw.groundcover_files),
                    "fallback_archives": list(component.openmw.fallback_archives),
                    "fallback_entries": dict(component.openmw.fallback_entries),
                    "config": component.openmw.config,
                    "requires_content": list(component.openmw.requires_content),
                }
                for component in project.components
            },
            "requires_content": list(project.openmw.requires_content),
            "settings": [
                {"category": setting.category, "key": setting.key, "value": setting.value}
                for setting in project.openmw.settings
            ],
        }
        if project.openmw.lua_api:
            openmw["lua_api"] = str(project.openmw.lua_api)
        semantics["extensions"]["openmw"] = openmw
    semantics["extensions"].update(project.extensions)
    semantics["critical_extensions"] = ["openmw"] if "openmw" in project.runtimes else []
    return semantics


def relationship_document(relationship) -> dict:
    document: dict = {"kind": relationship.kind}
    if relationship.project_id:
        document["project"] = relationship.project_id
    if relationship.capability:
        document["capability"] = relationship.capability
    if relationship.name:
        document["name"] = relationship.name
    if relationship.version:
        document["version"] = str(relationship.version)
    if relationship.url:
        document["url"] = relationship.url
    if relationship.reason:
        document["reason"] = relationship.reason
    return document


def openmw_has_install_data(project: Project) -> bool:
    for component in project.components:
        openmw = component.openmw
        if openmw.content_files or openmw.groundcover_files or openmw.fallback_archives or openmw.fallback_entries or openmw.config or openmw.requires_content:
            return True
        if openmw.data_directories != ["."]:
            return True
    return bool(project.openmw.lua_api or project.openmw.requires_content or project.openmw.settings)


def check_openmw_runtime(project: Project, problems: Problems) -> None:
    if openmw_has_install_data(project) and "openmw" not in project.runtimes:
        problems.error(
            f"{project.directory}/mod.toml",
            'declares OpenMW install data but no OpenMW runtime; add [runtimes] openmw = ">=0.49" (or "*")',
        )


def payload_release_document(project: Project, version: Version, semantics: dict) -> bytes:
    """dreamweave.release.json, shipped inside the archive so a loose zip can identify itself."""
    document = {
        "schema_version": SCHEMA_VERSION,
        "document": "release-payload",
        "project": {"id": project.id, "name": project.name, "slug": project.slug, "versioning": project.versioning},
        "version": str(version),
        "format": project.package_format,
        **semantics,
    }
    return dumps(document).encode("utf-8")


def archive_filename(project: Project) -> str:
    return f"{project.slug}.zip"


@dataclass
class LockedRelease:
    version: Version
    locked_from: str
    artifacts: list[dict]
    semantics: dict

    def to_document(self) -> dict:
        return {
            "version": str(self.version),
            "locked_from": self.locked_from,
            "artifacts": self.artifacts,
            **self.semantics,
        }


def read_lock(project: Project, root: Path, problems: Problems) -> list[LockedRelease]:
    path = root / project.directory / MOD_LOCK
    where = f"{project.directory}/{MOD_LOCK}"
    if not path.is_file():
        return []
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        problems.error(where, f"is not valid JSON ({error}); CI writes it when a release tag is pushed")
        return []

    if document.get("schema_version") != SCHEMA_VERSION or document.get("document") != "lock":
        problems.error(where, f"is not a schema_version {SCHEMA_VERSION} lock document")
        return []
    if document.get("project") != project.id:
        problems.error(where, f"belongs to project {document.get('project')!r}, not {project.id!r}; a copied lock file claims someone else's releases")
        return []

    releases = []
    for index, record in enumerate(document.get("releases", [])):
        release = parse_locked_release(record, project, f"{where} releases[{index}]", problems)
        if release:
            releases.append(release)

    versions = [release.version for release in releases]
    for version in versions:
        if versions.count(version) > 1:
            problems.error(where, f"records version {version} more than once")
            break
    for release in releases:
        if not project.declared_release(release.version):
            problems.error(where, f"records {release.version}, which mod.toml does not declare in [[releases]]")
    return releases


def parse_locked_release(record: dict, project: Project, where: str, problems: Problems) -> LockedRelease | None:
    try:
        version = Version.parse(record.get("version"), project.versioning)
    except VersionError as error:
        problems.error(where, str(error))
        return None
    artifacts = record.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        problems.error(where, "has no artifacts")
        return None
    for artifact in artifacts:
        digest = (artifact.get("digests") or {}).get("sha256", "")
        if not (isinstance(digest, str) and len(digest) == 64 and all(character in "0123456789abcdef" for character in digest)):
            problems.error(where, f"artifact {artifact.get('id')!r} has a malformed sha256 digest {digest!r}")
        if not isinstance(artifact.get("size"), int) or artifact["size"] <= 0:
            problems.error(where, f"artifact {artifact.get('id')!r} has no valid size")
    missing = [key for key in SEMANTIC_KEYS if key not in record]
    if missing:
        problems.error(where, f"is missing {', '.join(missing)}")
        return None
    semantics = {key: record[key] for key in (*SEMANTIC_KEYS, "critical_extensions") if key in record}
    return LockedRelease(version=version, locked_from=record.get("locked_from", ""), artifacts=artifacts, semantics=semantics)


def write_lock(project: Project, root: Path, releases: list[LockedRelease]) -> Path:
    ordered = sorted(releases, key=lambda release: release.version.precedence_key())
    document = {
        "schema_version": SCHEMA_VERSION,
        "document": "lock",
        "project": project.id,
        "note": "Written by CI when a release tag is pushed: what each published release's archive contains. Do not edit.",
        "releases": [release.to_document() for release in ordered],
    }
    path = root / project.directory / MOD_LOCK
    path.write_text(dumps(document), encoding="utf-8")
    return path


@dataclass
class PublishedRelease:
    declared: DeclaredRelease | None
    locked: LockedRelease
    tag: str
    revision: str | None
    channel: str
    date: str | None


def artifact_sources(project: Project, site: SiteConfig, release_name: str, artifact: dict) -> list[dict]:
    filename = artifact["filename"]
    sources = [{
        "url": f"{site.repository_url}/releases/download/{release_name}/{filename}",
        "kind": "publisher",
    }]
    for mirror in project.mirrors:
        url = (
            mirror.url.replace("{slug}", project.slug)
            .replace("{version}", release_name.removeprefix(f"{project.slug}-"))
            .replace("{tag}", release_name)
            .replace("{filename}", filename)
            .replace("{sha256}", artifact["digests"]["sha256"])
        )
        source = {"url": url, "kind": "mirror"}
        if mirror.name:
            source["name"] = mirror.name
        sources.append(source)
    return sources


def artifact_signatures(project: Project, site: SiteConfig, release_name: str, artifact: dict, ref: str) -> list[dict]:
    if not project.sigstore:
        return []
    return [{
        "format": "sigstore-bundle",
        "url": f"{site.repository_url}/releases/download/{release_name}/{artifact['filename']}.sigstore.json",
        "issuer": SIGSTORE_ISSUER,
        "identity": f"{site.repository_url}/{WORKFLOW_PATH}@{ref}",
    }]


def notes_document(declared: DeclaredRelease | None) -> dict:
    if not declared:
        return {}
    notes = declared.notes
    document: dict = {}
    for key in ("summary", "highlights", "migration", "notes"):
        value = getattr(notes, key)
        if value:
            document[key] = value
    for key in ("added", "changed", "fixed", "breaking", "known_issues"):
        value = getattr(notes, key)
        if value:
            document[key] = list(value)
    return document


def release_document(project: Project, site: SiteConfig, release: PublishedRelease, release_name: str, ref: str) -> dict:
    declared = release.declared
    status = "available"
    document: dict = {
        "version": str(release.locked.version),
        "channel": release.channel,
    }
    if release.date:
        document["date"] = release.date
    if declared and declared.yanked:
        status = "yanked"
        document["yanked"] = {"reason": declared.yanked, **({"replacement": str(declared.replacement)} if declared.replacement else {})}
    elif declared and declared.deprecated:
        status = "deprecated"
        document["deprecated"] = {"reason": declared.deprecated, **({"replacement": str(declared.replacement)} if declared.replacement else {})}
    document["status"] = status

    source: dict = {"repository": site.repository_url}
    if release.channel == DEVELOPMENT_CHANNEL:
        source["release"] = release_name
    else:
        source["tag"] = release.tag
    if release.revision:
        source["revision"] = release.revision
    document["source"] = source

    notes = notes_document(declared)
    if notes:
        document["notes"] = notes

    document.update(release.locked.semantics)
    document["artifacts"] = [
        {
            **{key: artifact[key] for key in ARTIFACT_KEYS},
            **({"platform": artifact["platform"]} if "platform" in artifact else {}),
            **({"layout": artifact["layout"]} if "layout" in artifact else {}),
            "sources": artifact_sources(project, site, release_name, artifact),
            "signatures": artifact_signatures(project, site, release_name, artifact, ref),
        }
        for artifact in release.locked.artifacts
    ]
    return document


def channel_heads(releases: list[dict], scheme: str) -> dict:
    heads: dict = {}
    for release in releases:
        if release["status"] != "available":
            continue
        channel = release["channel"]
        version = Version.parse(release["version"], scheme)
        current = heads.get(channel)
        if current is None or version > Version.parse(current["version"], scheme):
            heads[channel] = {"version": release["version"]}
    return dict(sorted(heads.items()))


def project_links(project: Project, site: SiteConfig, base_url: str) -> dict:
    links = {"page": f"{base_url}/{project.page_path}"}
    links["source"] = project.links.get("source", site.repository_url)
    links["issues"] = project.links.get("issues", f"{site.repository_url}/issues")
    if project.package_crate:
        links["crate"] = f"https://crates.io/crates/{project.package_crate}"
    for key, value in project.links.items():
        if key == "documentation" and value.startswith("@/"):
            links[key] = f"{base_url}/{value.removeprefix('@/').removesuffix('_index.md').removesuffix('index.md')}"
        elif key not in links:
            links[key] = value
    return dict(sorted(links.items()))


def media_documents(project: Project, base_url: str) -> list[dict]:
    documents = []
    for item in project.media:
        document: dict = {"kind": item.kind, "alt": item.alt}
        if item.file:
            document["url"] = f"{base_url}/{project.page_path}{item.file}"
        if item.url:
            document["url"] = item.url
        if item.thumbnail:
            document["thumbnail"] = f"{base_url}/{project.page_path}{item.thumbnail}"
        for key in ("caption", "category"):
            value = getattr(item, key)
            if value:
                document[key] = value
        if item.featured:
            document["featured"] = True
        documents.append(document)
    return documents


def project_manifest(project: Project, site: SiteConfig, base_url: str, releases: list[dict]) -> dict:
    project_document: dict = {
        "id": project.id,
        "name": project.name,
    }
    if project.summary:
        project_document["summary"] = project.summary
    project_document.update({
        "type": project.type,
        "status": project.status,
        "versioning": project.versioning,
        "game": project.game,
    })
    if project.license:
        project_document["license"] = project.license
    project_document["tags"] = list(project.tags)
    project_document["maintainers"] = [{"name": person.name, **({"url": person.url} if person.url else {})} for person in project.maintainers]
    project_document["links"] = project_links(project, site, base_url)
    integrations = {}
    if project.nexusmods_mod_id is not None:
        integrations["nexusmods"] = {"game": project.game, "mod_id": project.nexusmods_mod_id}
    project_document["integrations"] = integrations
    project_document["media"] = media_documents(project, base_url)
    project_document["credits"] = [
        {"name": credit.name, **({"role": credit.role} if credit.role else {}), **({"url": credit.url} if credit.url else {})}
        for credit in project.credits
    ]

    ordered = sorted(releases, key=lambda release: Version.parse(release["version"], project.versioning).precedence_key(), reverse=True)
    return {
        "schema_version": SCHEMA_VERSION,
        "document": "project",
        "generator": GENERATOR,
        "project": project_document,
        "channels": channel_heads(ordered, project.versioning),
        "releases": ordered,
    }


def site_index(site: SiteConfig, base_url: str, entries: list[dict]) -> dict:
    return {
        "schema_version": SCHEMA_VERSION,
        "document": "index",
        "generator": GENERATOR,
        "site": {"name": site.title, "url": f"{base_url}/"},
        "projects": sorted(entries, key=lambda entry: entry["id"]),
    }
