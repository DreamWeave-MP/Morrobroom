+++
title = "Morrobroom Documentation"
description = "A short, practical manual for building OpenMW levels with TrenchBroom and Morrobroom."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"

[extra]
docs_root = true
docs_project_name = "Morrobroom"
docs_short_title = "Morrobroom Docs"
docs_project_path = "@/home/index.md"
docs_repository_url = "https://github.com/DreamWeave-MP/Morrobroom"
docs_sidebar_label = "Manual"
kind = "guide"
+++

**TrenchBroom is the editor. Morrobroom is the compiler. OpenMW is where you
play the result.**

This manual is deliberately small. It gets you from an empty install to a
working room, explains the parts of the workflow that are specific to
Morrowind, and gives you somewhere useful to look when the compiler objects to
your architectural decisions.

## Documentation map

- [Start here](@/docs/start-here.md) — install the tools, configure TrenchBroom,
  and compile your first room.
- [Mapping](@/docs/mapping.md) — the small vocabulary and authoring rules that
  matter when a Quake-style brush editor is making Morrowind geometry.
- [OpenMW integration](@/docs/openmw.md) — FGDs, game objects, plugins,
  generated assets, and baked lighting.
- [NIF import](@/docs/nif-import.md) — reverse-compile visual NIF geometry into
  editable Valve 220 brush geometry.
- [Tools and command line](@/docs/commands.md) — direct compiler commands for
  compiling maps, generating FGDs, and importing NIFs.
- [Troubleshooting](@/docs/troubleshooting.md) — the usual causes of missing
  entities, stale output, bad geometry, and lightmapping problems.

If you only want to make a room, read **Start here**, then return when a
specific problem gives you a reason. This is a manual, not a civic education
requirement.
