+++
title = "Manual"
description = "A short, practical manual for building OpenMW levels with TrenchBroom and Morrobroom."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"

[extra]
docs_root = true
docs_project_name = "Morrobroom"
docs_short_title = "Morrobroom manual"
docs_project_path = "@/home/index.md"
docs_repository_url = "https://github.com/DreamWeave-MP/Morrobroom/tree/main/content/docs"
docs_sidebar_label = "Manual"
hide_child_cards = true
kind = "manual"
+++

**TrenchBroom is the editor. Morrobroom is the compiler. OpenMW is where you play the result.**

This manual is deliberately small. It gets you from an empty install to a working room, explains
the parts of the workflow that are specific to Morrowind, and gives you somewhere useful to look
when the compiler objects to your architectural decisions.

## Learn it

- **[Start here](@/docs/start-here.md)**: install the tools, set up TrenchBroom, and compile your
  first room. About twenty minutes, most of it downloads.
- **[Mapping](@/docs/mapping.md)**: the small vocabulary and the authoring rules that matter when
  a Quake-style brush editor is making Morrowind geometry.

## Look it up

- **[Entities](@/docs/entities.md)**: everything you can place, what each one compiles into, and
  what does not compile yet.
- **[NIF authoring](@/docs/nif-authoring.md)**: materials, transparency, texturing, scrolling
  textures, billboards, sort nodes and collision, set from TrenchBroom.
- **[OpenMW integration](@/docs/openmw.md)**: what a compile writes, baked lighting, scale,
  plugins, and the entity catalog for your load order.
- **[NIF import](@/docs/nif-import.md)**: reverse-compile visual NIF geometry into editable brushes.
- **[Command line](@/docs/commands.md)**: every command and option, for when the TrenchBroom
  profile is not enough.
- **[Troubleshooting](@/docs/troubleshooting.md)**: the usual causes of missing entities, stale
  output, bad geometry and lighting problems, and what to send when it is none of those.

If you only want to make a room, read **Start here**, then come back when a specific problem gives
you a reason. This is a manual, not a civic education requirement.
