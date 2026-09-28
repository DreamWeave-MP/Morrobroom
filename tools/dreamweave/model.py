"""The project model: `content/<project>/mod.toml` plus the page's frontmatter, validated.

`mod.toml` owns structured facts. The page frontmatter owns the display name (`title`) and
summary (`description`). Nothing is stated in both places.
"""

import re
import tomllib
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath

import yaml

from .problems import Problems
from .tables import EXTENSION_NAMESPACE_PATTERN, TOKEN_PATTERN, Table, check_url, check_uuid
from .versions import DECIMAL, NUMERIC, SCHEMES, Constraint, Version

MOD_TOML = "mod.toml"
MOD_LOCK = "mod.lock"

SLUG_PATTERN = re.compile(r"^[a-z0-9][a-z0-9_]*$")
GITHUB_OWNER_PATTERN = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})$")
GITHUB_REPOSITORY_PATTERN = re.compile(r"^[A-Za-z0-9._-]{1,100}$")
PALETTES = ("purple", "teal", "gold", "ember", "moss", "umber")
DIRECTORY_PATTERN = re.compile(r"^[a-z0-9][a-z0-9_-]*$")
COMPONENT_ID_PATTERN = TOKEN_PATTERN
CAPABILITY_PATTERN = re.compile(r"^[a-z0-9][a-z0-9.-]*(:[a-z0-9][a-z0-9.-]*)?$")
CHANNEL_PATTERN = TOKEN_PATTERN
DEVELOPMENT_CHANNEL = "development"

PROJECT_TYPES = ("mod", "library", "framework", "tool", "assets", "total-conversion", "documentation")
PROJECT_STATUSES = ("active", "maintenance", "experimental", "deprecated", "archived")
PACKAGE_FORMATS = ("flat", "bain", "fomod", "binary", "crate")
# Built by StroggForge's Rust workflows rather than zipped from the project directory.
RUST_FORMATS = ("binary", "crate")
GROUP_SELECTIONS = ("exactly-one", "at-most-one", "at-least-one", "any")
LINK_KEYS = ("source", "issues", "documentation", "support", "donate", "nexusmods", "homepage")
RELATIONSHIP_KINDS = ("requires", "recommends", "conflicts", "compatible", "replaces")
PLATFORM_SYSTEMS = ("windows", "macos", "linux")
PLATFORM_ARCHITECTURES = ("x86_64", "aarch64")

# A binary project's archives come from StroggForge's Rust workflow, one per platform, named
# <binary>-<runner OS>-<runner architecture>.zip: morrobroom-Windows-X64.zip.
BINARY_SYSTEM_NAMES = {"windows": "Windows", "macos": "macOS", "linux": "Linux"}
BINARY_ARCHITECTURE_NAMES = {"x86_64": "X64", "aarch64": "ARM64"}
BINARY_NAME_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")
CRATE_NAME_PATTERN = re.compile(r"^[A-Za-z][A-Za-z0-9_-]{0,63}$")
GITHUB_REPOSITORY_NAME_PATTERN = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})/[A-Za-z0-9._-]{1,100}$")

# Extension namespaces this template defines. Anything else must be dotted (org.tes3mp).
KNOWN_EXTENSION_NAMESPACES = ("openmw",)

CONTENT_FILE_SUFFIXES = (".esm", ".esp", ".omwgame", ".omwaddon", ".omwscripts")
GROUNDCOVER_FILE_SUFFIXES = (".esp", ".omwaddon")
ARCHIVE_FILE_SUFFIXES = (".bsa",)
MEDIA_IMAGE_SUFFIXES = (".png", ".jpg", ".jpeg", ".webp", ".gif", ".avif")
WINDOWS_RESERVED_CHARACTERS = set('<>:"|?*\\')

# Identities used by this repository's own examples. A project created from the template
# must not claim them, or two unrelated mods would share an identity on the network.
EXAMPLE_PROJECT_IDS = {
    "4d0c9f6e-2b1a-4c8e-9f3a-7e5d1b2c6a90": "content/home",
    "9b7e3f21-6c4d-4a8b-b1e2-3f5a7c9d0e14": "content/simplified",
}
TEMPLATE_REPOSITORY = "DreamWeave-MP/DreamWeave-Mod-Template"

# The GitHub release (and tag) the development build is published under. The template's own
# repository once had immutable releases turned on while it published `development`, and GitHub
# never lets a tag name used by an immutable release be used again. So the template publishes
# under another name, and every site made from it keeps `development`.
DEVELOPMENT_RELEASE = "development"
TEMPLATE_DEVELOPMENT_RELEASE = "dev-build"


@dataclass
class Relationship:
    kind: str
    project_id: str | None
    capability: str | None
    name: str | None
    version: Constraint | None
    url: str | None
    reason: str | None


@dataclass
class OpenMWComponent:
    data_directories: list[str]
    content_files: list[str]
    groundcover_files: list[str]
    fallback_archives: list[str]
    fallback_entries: dict[str, str]
    config: bool
    requires_content: list[str]


@dataclass
class Component:
    id: str
    name: str
    path: str
    description: str | None
    required: bool
    default: bool
    group: str | None
    requires: list[str]
    conflicts: list[str]
    suggested_with: list[str]
    openmw: OpenMWComponent


@dataclass
class Group:
    id: str
    name: str
    selection: str
    description: str | None


@dataclass
class OpenMWSetting:
    category: str
    key: str
    value: str


@dataclass
class OpenMWProject:
    lua_api: Constraint | None
    requires_content: list[str]
    settings: list[OpenMWSetting]


@dataclass
class Media:
    kind: str
    file: str | None
    url: str | None
    alt: str
    caption: str | None
    category: str | None
    featured: bool
    thumbnail: str | None


@dataclass
class Credit:
    name: str
    role: str | None
    url: str | None


@dataclass
class Person:
    name: str
    url: str | None


@dataclass
class ReleaseNotes:
    summary: str | None
    highlights: str | None
    added: list[str]
    changed: list[str]
    fixed: list[str]
    breaking: list[str]
    migration: str | None
    known_issues: list[str]
    notes: str | None


@dataclass
class DeclaredRelease:
    version: Version
    channel: str
    date: str
    notes: ReleaseNotes
    yanked: str | None
    deprecated: str | None
    replacement: Version | None


@dataclass
class Mirror:
    name: str | None
    url: str


@dataclass
class Platform:
    system: str
    architecture: str

    @property
    def id(self) -> str:
        """A token for artifact ids: linux-x64, macos-arm64."""
        return f"{self.system}-{BINARY_ARCHITECTURE_NAMES[self.architecture].lower()}"


@dataclass
class Project:
    directory: str
    id: str
    slug: str
    name: str
    summary: str | None
    tags: list[str]
    type: str
    status: str
    versioning: str
    game: str
    license: str | None
    maintainers: list[Person]
    links: dict[str, str]
    runtimes: dict[str, Constraint]
    platforms: list[Platform]
    provides: list[str]
    relationships: list[Relationship]
    components: list[Component]
    groups: list[Group]
    implicit_component: bool
    openmw: OpenMWProject
    package_format: str
    package_documentation: bool
    package_development: bool
    package_binary: str | None
    package_include: list[str]
    package_crate: str | None
    build_dependents: list[str]
    build_benchmarks: bool
    install_notes: dict[str, str]
    media: list[Media]
    credits: list[Credit]
    mirrors: list[Mirror]
    nexusmods_mod_id: int | None
    nexusmods_file_group_id: str | None
    sigstore: bool
    releases: list[DeclaredRelease]
    extensions: dict[str, dict]

    @property
    def page_path(self) -> str:
        return str(PurePosixPath(self.directory).relative_to("content")) + "/"

    def binary_archive(self, platform: Platform) -> str:
        """The archive StroggForge builds for one platform of a binary project."""
        return f"{self.package_binary}-{BINARY_SYSTEM_NAMES[platform.system]}-{BINARY_ARCHITECTURE_NAMES[platform.architecture]}.zip"

    def release_tag(self, version: Version) -> str:
        # A crate keeps the plain version tags crates are released under; there is one per repository.
        if self.package_format == "crate":
            return str(version)
        return f"{self.slug}-{version}"

    def declared_release(self, version: Version) -> DeclaredRelease | None:
        for release in self.releases:
            if release.version == version:
                return release
        return None


@dataclass
class SiteConfig:
    title: str
    base_url: str
    repository_owner: str
    repository_name: str
    extra: dict = field(default_factory=dict)

    @property
    def repository(self) -> str:
        return f"{self.repository_owner}/{self.repository_name}"

    @property
    def repository_url(self) -> str:
        return f"https://github.com/{self.repository}"

    @property
    def development_release(self) -> str:
        return TEMPLATE_DEVELOPMENT_RELEASE if self.repository.lower() == TEMPLATE_REPOSITORY.lower() else DEVELOPMENT_RELEASE


def load_site_config(root: Path, problems: Problems) -> SiteConfig:
    config = tomllib.loads((root / "config.toml").read_text(encoding="utf-8"))
    extra = config.get("extra", {})
    owner = extra.get("github_username")
    name = extra.get("github_project")
    if not isinstance(owner, str) or not owner or not isinstance(name, str) or not name:
        problems.error("config.toml [extra]", "github_username and github_project must name the repository that publishes this site")
    elif not GITHUB_OWNER_PATTERN.match(owner) or not GITHUB_REPOSITORY_PATTERN.match(name):
        problems.error("config.toml [extra]", f"{owner}/{name} is not a GitHub repository name")
    palette = extra.get("palette", "purple")
    if palette not in PALETTES:
        problems.error("config.toml [extra] palette", f"{palette!r} is not one of {', '.join(PALETTES)}; recolor further with accent or sass/brand.sass")
    return SiteConfig(
        title=config.get("title", ""),
        base_url=config.get("base_url", "").rstrip("/"),
        repository_owner=owner or "",
        repository_name=name or "",
        extra=extra,
    )


def read_frontmatter(path: Path) -> dict | None:
    text = path.read_text(encoding="utf-8")
    if text.startswith("+++"):
        parts = text.split("+++", 2)
        if len(parts) == 3:
            return tomllib.loads(parts[1])
    if text.startswith("---"):
        parts = text.split("---", 2)
        if len(parts) == 3:
            loaded = yaml.safe_load(parts[1])
            return loaded if isinstance(loaded, dict) else {}
    return None


def discover_project_directories(root: Path) -> list[Path]:
    content = root / "content"
    return sorted(path.parent for path in content.rglob(MOD_TOML) if path.is_file())


def load_project(root: Path, directory: Path, problems: Problems) -> Project | None:
    relative_directory = directory.relative_to(root).as_posix()
    where = f"{relative_directory}/{MOD_TOML}"

    for part in PurePosixPath(relative_directory).parts[1:]:
        if not DIRECTORY_PATTERN.match(part):
            problems.error(
                relative_directory,
                f"directory name {part!r} becomes part of the project URL; use lowercase letters, digits, '_' and '-'",
            )
            return None

    index_path = directory / "index.md"
    if not index_path.is_file():
        problems.error(relative_directory, "a project directory needs an index.md page next to mod.toml")
        return None

    try:
        frontmatter = read_frontmatter(index_path)
    except (tomllib.TOMLDecodeError, yaml.YAMLError) as error:
        problems.error(f"{relative_directory}/index.md", f"frontmatter does not parse: {error}")
        return None
    if frontmatter is None:
        problems.error(f"{relative_directory}/index.md", "needs frontmatter between +++ (TOML) or --- (YAML) fences")
        return None

    try:
        data = tomllib.loads((directory / MOD_TOML).read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as error:
        problems.error(where, f"does not parse as TOML: {error}")
        return None

    page_where = f"{relative_directory}/index.md"
    name = frontmatter.get("title")
    if not isinstance(name, str) or not name.strip():
        problems.error(page_where, "frontmatter `title` is the project's display name and is required")
        name = ""
    summary = frontmatter.get("description")
    if summary is not None and not isinstance(summary, str):
        problems.error(page_where, "frontmatter `description` must be a string")
        summary = None
    for key in ("path", "slug"):
        if key in frontmatter:
            problems.error(page_where, f"frontmatter `{key}` would move the project page away from its directory; rename the directory instead")
    if "version" in (frontmatter.get("extra") or {}):
        problems.error(page_where, "extra.version is the legacy release field; with mod.toml, declare releases in [[releases]]")
    if "install_info" in (frontmatter.get("extra") or {}):
        problems.error(page_where, "extra.install_info is the legacy install field; with mod.toml, use [openmw] or [[components]]")

    tags = []
    taxonomies = frontmatter.get("taxonomies") or {}
    if isinstance(taxonomies, dict) and isinstance(taxonomies.get("tags"), list):
        tags = [str(tag) for tag in taxonomies["tags"]]

    table = Table(data, where, problems)
    project = read_project(table, relative_directory, name, summary, tags, problems)
    table.finish()
    return project


def read_project(table: Table, directory: str, name: str, summary: str | None, tags: list[str], problems: Problems) -> Project:
    project_id = table.uuid("id")
    slug = table.string("slug", pattern=SLUG_PATTERN, describe="a slug: lowercase letters, digits and '_' (no '-': release tags are <slug>-<version>)")
    project_type = table.choice("type", PROJECT_TYPES, "mod")
    status = table.choice("status", PROJECT_STATUSES, "active")
    versioning = table.choice("versioning", SCHEMES, NUMERIC)
    game = table.string("game", "morrowind", pattern=TOKEN_PATTERN, describe="a lowercase game token like morrowind")
    license_expression = table.string("license", None)

    maintainers = [read_person(item) for item in table.table_list("maintainers")]

    links_table = table.table("links")
    links = {}
    for key in LINK_KEYS:
        if key == "documentation" and isinstance(links_table.data.get(key), str) and links_table.data[key].startswith("@/"):
            links[key] = links_table.string(key)
            continue
        value = links_table.url(key, None)
        if value:
            links[key] = value
    links_table.finish()

    runtimes_table = table.table("runtimes")
    runtimes = {}
    for runtime in sorted(runtimes_table.data):
        if not TOKEN_PATTERN.match(runtime):
            problems.error(runtimes_table.child_where(runtime), "runtime ids are lowercase tokens like openmw")
            runtimes_table.seen.add(runtime)
            continue
        constraint = runtimes_table.constraint(runtime)
        if constraint:
            runtimes[runtime] = constraint
    runtimes_table.finish()

    platforms = []
    for platform_table in table.table_list("platforms"):
        platforms.append(Platform(
            system=platform_table.choice("os", PLATFORM_SYSTEMS, "linux"),
            architecture=platform_table.choice("arch", PLATFORM_ARCHITECTURES, "x86_64"),
        ))
        platform_table.finish()

    provides = table.string_list("provides", pattern=CAPABILITY_PATTERN, describe="a capability name like music-playlists or dreamweave:music-playlists")

    relationships = []
    for kind in RELATIONSHIP_KINDS:
        for relationship_table in table.table_list(kind):
            relationship = read_relationship(kind, relationship_table, problems)
            relationship_table.finish()
            if relationship:
                relationships.append(relationship)

    groups = []
    for group_table in table.table_list("groups"):
        groups.append(Group(
            id=group_table.string("id", pattern=COMPONENT_ID_PATTERN, describe="a lowercase token") or "",
            name=group_table.string("name") or "",
            selection=group_table.choice("select", GROUP_SELECTIONS, "any"),
            description=group_table.string("description", None),
        ))
        group_table.finish()

    openmw_table = table.table("openmw")
    openmw = OpenMWProject(
        lua_api=openmw_table.constraint("lua_api", None),
        requires_content=openmw_table.string_list("requires_content"),
        settings=[],
    )
    for setting_table in openmw_table.table_list("settings"):
        value = setting_table.raw("value")
        if isinstance(value, bool):
            value = "true" if value else "false"
        elif isinstance(value, (int, float)):
            value = str(value)
        elif not isinstance(value, str):
            problems.error(setting_table.child_where("value"), "must be a string, number or boolean")
            value = ""
        openmw.settings.append(OpenMWSetting(
            category=setting_table.string("category") or "",
            key=setting_table.string("key") or "",
            value=value,
        ))
        setting_table.finish()

    component_tables = table.table_list("components")
    implicit_component = not component_tables
    components = []
    if implicit_component:
        components.append(Component(
            id="main",
            name=name or "Main",
            path=".",
            description=None,
            required=True,
            default=True,
            group=None,
            requires=[],
            conflicts=[],
            suggested_with=[],
            openmw=read_openmw_component(openmw_table, component_level=False),
        ))
    else:
        for component_table in component_tables:
            components.append(read_component(component_table, problems))
            component_table.finish()
    openmw_table.finish()

    package_table = table.table("package")
    package_format = package_table.choice("format", PACKAGE_FORMATS, "flat")
    documentation_declared = package_table.has("documentation")
    package_documentation = package_table.boolean("documentation", package_format != "binary")
    package_development = package_table.boolean("development", True)
    package_binary = package_table.string("binary", None, pattern=BINARY_NAME_PATTERN, describe="a Cargo binary name")
    package_include = package_table.string_list("include")
    package_crate = package_table.string("crate", None, pattern=CRATE_NAME_PATTERN, describe="a crates.io package name")
    package_table.finish()
    if package_format == "crate":
        if not package_crate:
            problems.error(package_table.child_where("crate"), 'a crate package names its crates.io package, like crate = "openmw-config"')
        if documentation_declared and package_documentation:
            problems.error(package_table.child_where("documentation"), "a crate has no archive to put documentation in; the site is its documentation")
        if package_table.has("development") and package_development:
            problems.error(package_table.child_where("development"), "a crate has no development build: its users depend on a published version, or on the repository itself")
        package_documentation = False
        package_development = False
    elif package_crate:
        problems.error(package_table.child_where("crate"), 'only format = "crate" packages have a crate')

    build_table = table.table("build")
    build_dependents = build_table.string_list("dependents", pattern=GITHUB_REPOSITORY_NAME_PATTERN, describe="a GitHub repository, owner/name")
    build_benchmarks = build_table.boolean("benchmarks", False)
    build_table.finish()
    if (build_dependents or build_benchmarks) and package_format not in RUST_FORMATS:
        problems.error(build_table.where, 'only projects StroggForge builds (format = "binary" or "crate") take [build] settings')
    if package_format == "binary":
        if not package_binary:
            problems.error(package_table.child_where("binary"), 'a binary package names the Cargo binary its archives hold, like binary = "morrobroom"')
        if documentation_declared and package_documentation:
            problems.error(package_table.child_where("documentation"), "a binary package's archives are built by the Rust workflow, which cannot add the rendered docs; list the docs in include instead")
    else:
        if package_binary:
            problems.error(package_table.child_where("binary"), 'only format = "binary" packages have a binary')
        if package_include:
            problems.error(package_table.child_where("include"), 'only format = "binary" packages include extra files; every other format ships the project directory')

    install_table = table.table("install")
    install_notes = {}
    for key in ("notes", "post_install", "upgrade", "uninstall"):
        value = install_table.string(key, None)
        if value:
            install_notes[key] = value
    install_table.finish()

    media = []
    for media_table in table.table_list("media"):
        item = read_media(media_table, problems)
        media_table.finish()
        if item:
            media.append(item)

    credits = []
    for credit_table in table.table_list("credits"):
        credits.append(Credit(
            name=credit_table.string("name") or "",
            role=credit_table.string("role", None),
            url=credit_table.url("url", None),
        ))
        credit_table.finish()

    mirrors = []
    for mirror_table in table.table_list("mirrors"):
        mirror_url = mirror_table.string("url")
        if mirror_url:
            check_mirror_template(mirror_url, mirror_table.child_where("url"), problems)
        mirrors.append(Mirror(name=mirror_table.string("name", None), url=mirror_url or ""))
        mirror_table.finish()

    nexus_table = table.table("nexusmods")
    nexus_mod_id = nexus_table.integer("mod_id", None)
    nexus_file_group_id = nexus_table.raw("file_group_id", None)
    if nexus_file_group_id is not None:
        nexus_file_group_id = str(nexus_file_group_id)
    nexus_table.finish()

    provenance_table = table.table("provenance")
    sigstore = provenance_table.boolean("sigstore", False)
    provenance_table.finish()

    releases = []
    for release_table in table.table_list("releases"):
        release = read_release(release_table, problems, versioning)
        release_table.finish()
        if release:
            releases.append(release)

    extensions_table = table.table("extensions")
    extensions = {}
    for namespace in sorted(extensions_table.data):
        extensions_table.seen.add(namespace)
        value = extensions_table.data[namespace]
        where = extensions_table.child_where(namespace)
        if namespace in KNOWN_EXTENSION_NAMESPACES:
            problems.error(where, f"the {namespace} extension is written as a top-level [{namespace}] table, not under [extensions]")
        elif not EXTENSION_NAMESPACE_PATTERN.match(namespace):
            problems.error(where, "third-party extension namespaces must be dotted and owned by someone, like org.tes3mp or io.github.someone.tool")
        elif not isinstance(value, dict):
            problems.error(where, "an extension must be a table")
        else:
            extensions[namespace] = to_json_value(value)
    extensions_table.finish()

    project = Project(
        directory=directory,
        id=project_id or "",
        slug=slug or "",
        name=name,
        summary=summary,
        tags=tags,
        type=project_type,
        status=status,
        versioning=versioning,
        game=game or "morrowind",
        license=license_expression,
        maintainers=maintainers,
        links=links,
        runtimes=runtimes,
        platforms=platforms,
        provides=provides,
        relationships=relationships,
        components=components,
        groups=groups,
        implicit_component=implicit_component,
        openmw=openmw,
        package_format=package_format,
        package_documentation=package_documentation,
        package_development=package_development,
        package_binary=package_binary,
        package_include=package_include,
        package_crate=package_crate,
        build_dependents=build_dependents,
        build_benchmarks=build_benchmarks,
        install_notes=install_notes,
        media=media,
        credits=credits,
        mirrors=mirrors,
        nexusmods_mod_id=nexus_mod_id,
        nexusmods_file_group_id=nexus_file_group_id,
        sigstore=sigstore,
        releases=releases,
        extensions=extensions,
    )
    check_project_structure(project, f"{directory}/{MOD_TOML}", problems)
    return project


def read_person(table: Table) -> Person:
    person = Person(name=table.string("name") or "", url=table.url("url", None))
    table.finish()
    return person


def read_relationship(kind: str, table: Table, problems: Problems) -> Relationship | None:
    project_id = table.uuid("id", None)
    capability = table.string("capability", None, pattern=CAPABILITY_PATTERN, describe="a capability name")
    name = table.string("name", None)
    # The target's versioning scheme is in its own manifest, so only the syntax is checked here;
    # the decimal grammar is the more permissive of the two.
    version = table.constraint("version", None, scheme=DECIMAL)
    url = table.url("url", None)
    reason = table.string("reason", None)

    if project_id and capability:
        problems.error(table.where, "name a project `id` or a `capability`, not both")
    if capability and kind not in ("requires", "recommends", "conflicts"):
        problems.error(table.where, f"{kind} entries name projects; capabilities only make sense for requires, recommends and conflicts")
    if not project_id and not capability and not name:
        problems.error(table.where, "needs an `id` (a DreamWeave project), a `capability`, or at least a `name` for humans")
        return None
    if version and not project_id:
        problems.error(table.where, "a `version` constraint needs an `id`; a client cannot check versions of a project it cannot identify")

    return Relationship(kind=kind, project_id=project_id, capability=capability, name=name, version=version, url=url, reason=reason)


def read_openmw_component(table: Table, component_level: bool = True) -> OpenMWComponent:
    fallback_table = table.table("fallback_entries")
    fallback_entries = {}
    for key in sorted(fallback_table.data):
        value = fallback_table.string(key)
        if value is not None:
            fallback_entries[key] = value
    fallback_table.finish()

    return OpenMWComponent(
        data_directories=table.string_list("data_directories", ["."]),
        content_files=table.string_list("content_files"),
        groundcover_files=table.string_list("groundcover_files"),
        fallback_archives=table.string_list("fallback_archives"),
        fallback_entries=fallback_entries,
        config=table.boolean("config", False),
        requires_content=table.string_list("requires_content") if component_level else [],
    )


def read_component(table: Table, problems: Problems) -> Component:
    component_id = table.string("id", pattern=COMPONENT_ID_PATTERN, describe="a lowercase token like core or hd-textures") or ""
    required = table.boolean("required", False)
    default = table.boolean("default", required)
    openmw_table = table.table("openmw")
    component = Component(
        id=component_id,
        name=table.string("name") or component_id,
        path=table.string("path") or "",
        description=table.string("description", None),
        required=required,
        default=default,
        group=table.string("group", None),
        requires=table.string_list("requires"),
        conflicts=table.string_list("conflicts"),
        suggested_with=table.string_list("suggested_with"),
        openmw=read_openmw_component(openmw_table),
    )
    openmw_table.finish()
    return component


def read_media(table: Table, problems: Problems) -> Media | None:
    file = table.string("file", None)
    video = table.url("video", None)
    if bool(file) == bool(video):
        problems.error(table.where, "set exactly one of `file` (an image next to mod.toml) or `video` (a URL)")
    return Media(
        kind="video" if video else "image",
        file=file,
        url=video,
        alt=table.string("alt") or "",
        caption=table.string("caption", None),
        category=table.string("category", None),
        featured=table.boolean("featured", False),
        thumbnail=table.string("thumbnail", None),
    )


def read_release(table: Table, problems: Problems, scheme: str) -> DeclaredRelease | None:
    version = table.version("version", scheme)
    channel = table.string("channel", "stable", pattern=CHANNEL_PATTERN, describe="a lowercase channel name like stable or beta")
    date = table.date("date")
    replacement = None
    if table.has("replacement"):
        replacement = table.version("replacement", scheme)

    notes = ReleaseNotes(
        summary=table.string("summary", None),
        highlights=table.string("highlights", None),
        added=table.string_list("added"),
        changed=table.string_list("changed"),
        fixed=table.string_list("fixed"),
        breaking=table.string_list("breaking"),
        migration=table.string("migration", None),
        known_issues=table.string_list("known_issues"),
        notes=table.string("notes", None),
    )
    yanked = table.string("yanked", None)
    deprecated = table.string("deprecated", None)

    if channel == DEVELOPMENT_CHANNEL:
        problems.error(table.child_where("channel"), "the development channel is built from your default branch automatically; do not declare its releases")
    if yanked and deprecated:
        problems.error(table.where, "a release is either yanked or deprecated, not both")
    if replacement and not (yanked or deprecated):
        problems.error(table.child_where("replacement"), "only yanked or deprecated releases have a replacement")
    if version and "+" in version.text:
        problems.error(table.child_where("version"), "release versions cannot carry +build metadata; it does not change precedence, so two releases could collide")

    if not version or not date:
        return None
    return DeclaredRelease(version=version, channel=channel or "stable", date=date, notes=notes, yanked=yanked, deprecated=deprecated, replacement=replacement)


def check_mirror_template(template: str, where: str, problems: Problems) -> None:
    placeholders = set(re.findall(r"\{([^}]*)\}", template))
    unknown = placeholders - {"slug", "version", "filename", "sha256", "tag"}
    if unknown:
        problems.error(where, f"unknown placeholder(s) {', '.join(sorted(unknown))}; use {{slug}}, {{version}}, {{tag}}, {{filename}} or {{sha256}}")
    if "filename" not in placeholders and "sha256" not in placeholders:
        problems.error(where, "a mirror URL must include {filename} or {sha256}, or every artifact would share one URL")
    check_url(template.replace("{", "").replace("}", ""), where, problems)


def check_relative_path(path: str, where: str, problems: Problems) -> bool:
    if path == ".":
        return True
    pure = PurePosixPath(path)
    if not path or path.startswith("/") or "\\" in path or pure.is_absolute():
        problems.error(where, f"{path!r} must be a relative path with forward slashes")
        return False
    if any(part in ("..", ".", "") for part in path.split("/")):
        problems.error(where, f"{path!r} may not contain '.', '..' or empty segments")
        return False
    return True


def check_project_structure(project: Project, where: str, problems: Problems) -> None:
    component_ids = [component.id for component in project.components]
    group_ids = [group.id for group in project.groups]
    for label, identifiers in (("component", component_ids), ("group", group_ids)):
        duplicates = sorted({identifier for identifier in identifiers if identifiers.count(identifier) > 1})
        for duplicate in duplicates:
            problems.error(where, f"{label} id {duplicate!r} is used more than once")

    component_paths = [component.path for component in project.components]
    for duplicate in sorted({path for path in component_paths if component_paths.count(path) > 1}):
        problems.error(where, f"component path {duplicate!r} is used by more than one component")

    for component in project.components:
        component_where = f"{where} component {component.id!r}"
        check_relative_path(component.path, component_where, problems)
        if project.package_format == "flat" and component.path != ".":
            problems.error(component_where, "a flat package is one data directory; its only component has path \".\" (use format = \"bain\" for numbered directories)")
        if project.package_format in ("bain", "fomod") and (component.path == "." or "/" in component.path):
            problems.error(component_where, f"a {project.package_format} package's components are top-level directories like \"00 Core\"; got {component.path!r}")
        if component.group and component.group not in group_ids:
            problems.error(component_where, f"group {component.group!r} is not declared in [[groups]]")
        if component.required and component.group:
            problems.error(component_where, "a required component is always installed, so it cannot be one choice in a group")
        for reference_kind in ("requires", "conflicts"):
            for reference in getattr(component, reference_kind):
                if reference not in component_ids:
                    problems.error(component_where, f"{reference_kind} unknown component {reference!r}")
                if reference == component.id:
                    problems.error(component_where, f"cannot {reference_kind} itself")
        for both in sorted(set(component.requires) & set(component.conflicts)):
            problems.error(component_where, f"both requires and conflicts with {both!r}")
        for project_reference in component.suggested_with:
            check_uuid(project_reference, f"{component_where} suggested_with", problems)
        for directory in component.openmw.data_directories:
            check_relative_path(directory, f"{component_where} openmw.data_directories", problems)
        check_openmw_files(component, component_where, problems)

    if project.package_format == "flat" and len(project.components) != 1:
        problems.error(where, "a flat package has exactly one component; use format = \"bain\" or \"fomod\" for several")

    required_ids = {component.id for component in project.components if component.required}
    for component in project.components:
        if component.required:
            for conflict in component.conflicts:
                if conflict in required_ids:
                    problems.error(where, f"required components {component.id!r} and {conflict!r} conflict, so nothing can be installed")

    for group in project.groups:
        members = [component for component in project.components if component.group == group.id]
        group_where = f"{where} group {group.id!r}"
        if not members:
            problems.error(group_where, "has no components")
        defaults = [component for component in members if component.default]
        if group.selection in ("exactly-one", "at-most-one") and len(defaults) > 1:
            problems.error(group_where, f"selects {group.selection} but {len(defaults)} members default to installed")
        if group.selection == "exactly-one" and len(defaults) != 1 and members:
            problems.error(group_where, "selects exactly-one, so exactly one member needs default = true")

    seen_relationships = set()
    for relationship in project.relationships:
        key = (relationship.kind, relationship.project_id, relationship.capability, relationship.name if not relationship.project_id else None)
        if key in seen_relationships:
            problems.error(where, f"[[{relationship.kind}]] lists {relationship.project_id or relationship.capability or relationship.name!r} twice")
        seen_relationships.add(key)
        if relationship.project_id and relationship.project_id == project.id:
            problems.error(where, f"[[{relationship.kind}]] cannot name the project itself")
    required_ids_external = {relationship.project_id for relationship in project.relationships if relationship.kind == "requires" and relationship.project_id}
    for relationship in project.relationships:
        if relationship.kind == "conflicts" and relationship.project_id in required_ids_external:
            problems.error(where, f"project {relationship.project_id} is both required and a conflict")

    versions = [release.version for release in project.releases]
    for index, version in enumerate(versions):
        for other in versions[index + 1:]:
            if version == other:
                problems.error(where, f"releases {version} and {other} have the same precedence; versions must be unique")

    for release in project.releases:
        if release.replacement and not any(release.replacement == other.version for other in project.releases):
            problems.error(where, f"release {release.version} names replacement {release.replacement}, which is not declared")

    if project.package_format == "flat" and project.groups:
        problems.error(where, "a flat package has one component, so [[groups]] have nothing to choose between")

    if project.package_format == "binary":
        check_binary_package(project, where, problems)
    if project.package_format == "crate":
        check_crate_package(project, where, problems)

    if len([item for item in project.media if item.featured]) > 1:
        problems.error(where, "at most one [[media]] entry can be featured")


def check_binary_package(project: Project, where: str, problems: Problems) -> None:
    """A program built per platform: one archive per [[platforms]] entry, nothing installed into a game."""
    if not project.implicit_component:
        problems.error(where, "a binary package is one program per platform; it has no [[components]]")
    if project.groups:
        problems.error(where, "a binary package has no [[groups]]: there are no components to choose between")
    if not project.platforms:
        problems.error(where, 'a binary package lists the [[platforms]] it is built for, like os = "windows", arch = "x86_64"; each is one archive')
    seen = set()
    for platform in project.platforms:
        if platform.id in seen:
            problems.error(where, f"platform {platform.id} is listed twice")
        seen.add(platform.id)
    openmw = project.components[0].openmw if project.components else None
    if openmw and (openmw.content_files or openmw.groundcover_files or openmw.fallback_archives or openmw.fallback_entries or openmw.config or openmw.data_directories != ["."]):
        problems.error(where, "a binary package is a program, not data OpenMW loads; it has no [openmw] install data")
    if project.nexusmods_file_group_id is not None:
        problems.error(where, "a binary package has one archive per platform; Nexus Mods uploads for programs are StroggForge's (its NEXUS_GROUP_IDS secret), not [nexusmods] file_group_id")


def check_crate_package(project: Project, where: str, problems: Problems) -> None:
    """A Rust library, published to crates.io: no archive, nothing installed into a game."""
    if not project.implicit_component or project.groups:
        problems.error(where, "a crate package has no [[components]] or [[groups]]: it is one library")
    openmw = project.components[0].openmw if project.components else None
    if openmw and (openmw.content_files or openmw.groundcover_files or openmw.fallback_archives or openmw.fallback_entries or openmw.config or openmw.data_directories != ["."]):
        problems.error(where, "a crate package is a library, not data OpenMW loads; it has no [openmw] install data")
    if project.nexusmods_file_group_id is not None:
        problems.error(where, "a crate package is published to crates.io, not uploaded to Nexus Mods")
    if project.mirrors:
        problems.error(where, "a crate package is downloaded from crates.io; it has no [[mirrors]]")
    if project.platforms:
        problems.error(where, "a crate package is built by whoever depends on it; it has no [[platforms]]")


def check_openmw_files(component: Component, where: str, problems: Problems) -> None:
    for label, files, suffixes in (
        ("content_files", component.openmw.content_files, CONTENT_FILE_SUFFIXES),
        ("groundcover_files", component.openmw.groundcover_files, GROUNDCOVER_FILE_SUFFIXES),
        ("fallback_archives", component.openmw.fallback_archives, ARCHIVE_FILE_SUFFIXES),
    ):
        for file in files:
            if "/" in file or "\\" in file:
                problems.error(where, f"openmw.{label} entry {file!r} is a file name, not a path; OpenMW finds it in the data directories")
            elif not file.lower().endswith(suffixes):
                problems.error(where, f"openmw.{label} entry {file!r} should end in {', '.join(suffixes)}")


def to_json_value(value: object) -> object:
    if isinstance(value, dict):
        return {str(key): to_json_value(item) for key, item in value.items()}
    if isinstance(value, list):
        return [to_json_value(item) for item in value]
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return value.isoformat()
