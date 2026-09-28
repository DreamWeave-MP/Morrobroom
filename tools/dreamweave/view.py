"""static/dreamweave/view.json: what only CI knows, for the templates.

The templates build every project page from mod.toml, mod.lock and the page itself
(templates/macros/project.html), so a plain `zola serve` renders the whole site. This adds what
needs git, the network or a build: the development release, tags that were never recorded, the
comments embed and the network page's checks. In offline mode (documentation inside archives) it
only says which version is being packaged: never git state, mod.lock or the clock.
"""

import hashlib
import json

from . import records
from .build import GENERATED_ROOT, INDEX_FILE, Repository, release_state
from .model import DEVELOPMENT_CHANNEL, Project
from .payload import collect_payload, undeclared_content_files
from .problems import Problems
from .versions import Version


def diagnostic_checks(repository: Repository, project: Project, state, manifest: dict, archives_built: bool) -> list[dict]:
    checks = []

    def check(identifier: str, label: str, status: str, detail: str) -> None:
        checks.append({"id": identifier, "label": label, "state": status, "detail": detail})

    check("identity", "Stable identity", "pass", f"id {project.id}; survives renames and host moves")
    check("discovery", "Discovery", "pass", "listed in dreamweave.json and linked from every page with <link rel=\"alternate\">")
    check("manifest", "Manifest", "pass", f"schema_version {manifest['schema_version']}, {len(manifest['releases'])} release(s) published")

    stable = [release for release in state.published]
    if stable:
        check("releases", "Published releases", "pass", ", ".join(str(release.locked.version) for release in stable))
    else:
        check("releases", "Published releases", "warn", "no release tag has been pushed yet; clients only see the development channel")
    if state.unverified:
        check("unverified", "Tagged, never recorded", "warn", f"{', '.join(state.unverified)}: tagged before this template recorded releases; excluded from the manifest because nothing records their hashes")
    if state.planned:
        check("planned", "Declared, not tagged", "info", f"{', '.join(state.planned)}: push the tag <slug>-<version> to publish")

    channels = manifest["channels"]
    if channels:
        check("channels", "Channels", "pass", ", ".join(f"{channel} → {head['version']}" for channel, head in channels.items()))
    elif project.package_development and not archives_built:
        check("channels", "Channels", "info", "the development channel appears once CI packages it")
    else:
        check("channels", "Channels", "fail", "no channel has an available release")

    missing_digests = [
        f"{release['version']} {artifact['id']}"
        for release in manifest["releases"] for artifact in release["artifacts"]
        if "sha256" not in artifact.get("digests", {})
    ]
    if project.package_development and not archives_built:
        check("hashes", "Content hashes", "info", "development archives were not built in this build")
    elif missing_digests:
        check("hashes", "Content hashes", "fail", f"missing for {', '.join(missing_digests)}")
    else:
        check("hashes", "Content hashes", "pass", "every published artifact has a SHA-256 digest and size")

    identified = [relationship for relationship in project.relationships if relationship.project_id or relationship.capability]
    unidentified = [relationship for relationship in project.relationships if not relationship.project_id and not relationship.capability]
    if unidentified:
        names = ", ".join(relationship.name for relationship in unidentified if relationship.name)
        check("relationships", "Relationships", "info", f"{len(identified)} machine-resolvable; {names} named for humans only (no id)")
    elif project.relationships:
        check("relationships", "Relationships", "pass", f"{len(identified)} machine-resolvable")
    else:
        check("relationships", "Relationships", "info", "none declared")

    if project.sigstore:
        check("provenance", "Signatures", "pass", "CI signs each archive with Sigstore (keyless, tied to this repository's workflow)")
    else:
        check("provenance", "Signatures", "info", "not signed; hashes still verify integrity. Set [provenance] sigstore = true to add build provenance")

    revisions = [release for release in state.published if release.revision]
    check("source", "Source revisions", "pass" if revisions or not state.published else "warn", f"{len(revisions)} of {len(state.published)} published releases map to a commit")
    check("mirrors", "Mirrors", "info" if not project.mirrors else "pass", f"{len(project.mirrors)} configured; every artifact also has its publisher source")

    problems = Problems()
    payload = collect_payload(project, None, repository.nested_directories(project), problems, repository.root)
    undeclared = undeclared_content_files(project, payload)
    if undeclared:
        check("content", "Content files", "info", f"not declared in openmw.content_files: {', '.join(undeclared)}")
    return checks


def project_facts(repository: Repository, project: Project, offline_mode: bool, packaged: Version | None, archives_built: bool) -> dict:
    if offline_mode:
        return {"packaged_version": str(packaged)} if packaged else {}

    state = release_state(repository, project)
    facts: dict = {"unverified": state.unverified}
    manifest_path = repository.root / GENERATED_ROOT / "projects" / f"{project.id}.json"
    if not manifest_path.is_file():
        return facts
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    facts["manifest_sha256"] = hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    development = next((release for release in manifest["releases"] if release["channel"] == DEVELOPMENT_CHANNEL), None)
    if development:
        facts["development"] = development
    facts["checks"] = diagnostic_checks(repository, project, state, manifest, archives_built)
    return facts


def build_view(repository: Repository, base_url: str, offline_mode: bool, packaged_versions: dict[str, Version], archives_built: bool) -> dict:
    view: dict = {
        "offline": offline_mode,
        "generator": records.GENERATOR,
        "projects": {
            project.page_path: project_facts(repository, project, offline_mode, packaged_versions.get(project.id), archives_built)
            for project in repository.projects
        },
    }
    if offline_mode:
        return view

    view["index_url"] = f"{base_url}/{INDEX_FILE.name}"
    view["comments"] = repository.comments()
    view["archives_built"] = archives_built
    view["revision"] = repository.head
    return view
