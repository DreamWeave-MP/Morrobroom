"""FOMOD installer XML generated from the component model.

The package is still a BAIN tree; `fomod/` rides along for Mod Organizer 2 and Vortex. Each
component's data directories install to the mod root, which is where OpenMW (through MO2's
OpenMW support) expects a data directory. FOMOD cannot express one component requiring
another without flag plumbing, so component `requires` is enforced by DreamWeave clients only.
"""

from xml.sax.saxutils import escape, quoteattr

from .model import Component, Project
from .payload import join
from .versions import Version

SCHEMA_LOCATION = "http://qconsulting.ca/fo3/ModConfig5.0.xsd"
GROUP_TYPES = {
    "exactly-one": "SelectExactlyOne",
    "at-most-one": "SelectAtMostOne",
    "at-least-one": "SelectAtLeastOne",
    "any": "SelectAny",
}


def info_xml(project: Project, version: Version, website: str) -> bytes:
    authors = ", ".join(person.name for person in project.maintainers) or project.name
    machine_version = ".".join(version.release_text)
    lines = [
        '<?xml version="1.0" encoding="utf-8"?>',
        "<fomod>",
        f"  <Name>{escape(project.name)}</Name>",
        f"  <Author>{escape(authors)}</Author>",
        f"  <Version MachineVersion={quoteattr(machine_version)}>{escape(str(version))}</Version>",
    ]
    if project.summary:
        lines.append(f"  <Description>{escape(project.summary)}</Description>")
    lines.append(f"  <Website>{escape(website)}</Website>")
    lines.append("</fomod>")
    return ("\n".join(lines) + "\n").encode("utf-8")


def folder_elements(component: Component, priority: int, indent: str) -> list[str]:
    elements = []
    for directory in component.openmw.data_directories:
        source = join(component.path, directory).replace("/", "\\")
        elements.append(f'{indent}<folder source={quoteattr(source)} destination="" priority="{priority}" />')
    return elements


def plugin_element(component: Component, priority: int, indent: str) -> list[str]:
    description = component.description or component.name
    plugin_type = "Recommended" if component.default else "Optional"
    return [
        f"{indent}<plugin name={quoteattr(component.name)}>",
        f"{indent}  <description>{escape(description)}</description>",
        f"{indent}  <files>",
        *folder_elements(component, priority, indent + "    "),
        f"{indent}  </files>",
        f"{indent}  <typeDescriptor>",
        f'{indent}    <type name="{plugin_type}" />',
        f"{indent}  </typeDescriptor>",
        f"{indent}</plugin>",
    ]


def module_config_xml(project: Project) -> bytes:
    priorities = {component.id: index for index, component in enumerate(sorted(project.components, key=lambda component: component.path))}
    required = [component for component in project.components if component.required]
    optional = [component for component in project.components if not component.required]

    lines = [
        '<?xml version="1.0" encoding="utf-8"?>',
        f'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="{SCHEMA_LOCATION}">',
        f"  <moduleName>{escape(project.name)}</moduleName>",
    ]
    if required:
        lines.append("  <requiredInstallFiles>")
        for component in required:
            lines.extend(folder_elements(component, priorities[component.id], "    "))
        lines.append("  </requiredInstallFiles>")

    groups = []
    for group in project.groups:
        members = [component for component in optional if component.group == group.id]
        groups.append((group.name, GROUP_TYPES[group.selection], members))
    loose = [component for component in optional if component.group is None]
    if loose:
        groups.append(("Optional components", "SelectAny", loose))

    if groups:
        lines.append('  <installSteps order="Explicit">')
        lines.append('    <installStep name="Components">')
        lines.append('      <optionalFileGroups order="Explicit">')
        for name, group_type, members in groups:
            lines.append(f"        <group name={quoteattr(name)} type=\"{group_type}\">")
            lines.append('          <plugins order="Explicit">')
            for component in members:
                lines.extend(plugin_element(component, priorities[component.id], "            "))
            lines.append("          </plugins>")
            lines.append("        </group>")
        lines.append("      </optionalFileGroups>")
        lines.append("    </installStep>")
        lines.append("  </installSteps>")

    lines.append("</config>")
    return ("\n".join(lines) + "\n").encode("utf-8")
