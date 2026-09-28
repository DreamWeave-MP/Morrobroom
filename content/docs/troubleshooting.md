+++
title = "Troubleshooting"
description = "A short diagnostic guide for the problems you are most likely to meet, and what to send when it is none of them."
weight = 50

[extra]
kind = "diagnostics"
+++

## TrenchBroom does not list Morrowind

Check the custom game directory first. It must contain `GameConfig.cfg`,
`CompilationProfiles.cfg` and the rest of what was in Morrobroom's `resources/`, directly, not in
a `resources/` folder of its own. Restart TrenchBroom after copying them.

Then check the game directory setting. It must point to the folder containing `Data Files/`, not
to `Data Files/` itself.

## Textures are missing in TrenchBroom

Confirm that the configured Morrowind game directory contains `Data Files/`, and that the texture
exists under its `textures/` path as `.tga`, `.png`, `.dds` or `.webp`. Missing textures in the
editor are a TrenchBroom search-path problem, not a plugin problem.

## "No brushes found in map! You probably used an apostrophe in worldspawn properties"

The compiler means it. A property value containing `'` stops the map parser, and nothing after it
is read. Remove the apostrophe; for a cell named after Caius Cosades, `Caius Cosades House` will
have to do.

## OpenMW launches the old map

Save the `.map` before compiling. The compiler reads the saved file, not TrenchBroom's unsaved
state. Then check that the compile profile points OpenMW at `build/<map name>/` and loads the
matching `<map name>.omwaddon`.

## OpenMW does not start in the room

The Map-to-Engine profile starts OpenMW in the cell named after the map. If worldspawn has an
`ESM3_Name`, the cell is called that instead, and OpenMW has nowhere to start. Clear `ESM3_Name`,
or change the profile's `--start` to match.

## Something I placed is not in the game

Look at the compiler's output. `Unidentified point entity class: …` means the entity has no
compiled form yet: records placed from the generated catalog, `nif_fx_fire` and the VFX catalog are
all like that for now. `No matching object type found!` is the same for a brush entity.
[Entities](@/docs/entities.md) lists what does compile.

## Objects are missing from the entity browser

If a record from an installed mod does not appear, generate the catalog from the `openmw.cfg` that
loads it with `morrobroom fgd`, then reload the game configuration or restart TrenchBroom. Use
`--output <path>` for TrenchBroom's portable mode. The catalog shows records; placing them does not
compile yet, as above.

## Geometry fails or looks wrong

Start with a smaller map and isolate the newest brush or entity. Check for malformed or
zero-volume solids, degenerate faces, extremely thin slivers, and geometry imported from a NIF
rather than authored as a brush. Compile early and often; the compiler has no obligation to conceal
a cursed polygon until the end of the project.

For imported assets, run `nif2map --dry-run --verbose` first. Check the `.nif2map.json` report and
watch the brush count: complex visual geometry can produce very large `.map` files. See
[NIF import](@/docs/nif-import.md) for the structural and surface reconstruction model.

## "Nif_Target … does not match" and other link errors

A `Nif_Target` names a `Nif_LinkName` that is not in its scope, is in it twice, or leads round in a
circle. Inside a group, a target only sees markers in that group and the groups around it.
[NIF authoring](@/docs/nif-authoring.md#structure-groups-and-links) has the rules.

## Lighting is absent or stale

Make sure the map contains a light and that the compile was not run with `--no-lightmaps`. An
entity with its own `Nif_Texture_DarkMap` shows that texture instead of the lightmap. Delete the
map's `build/` directory if old generated textures make the result hard to read, then compile again.

## Reporting a problem

Include:

1. your operating system and Morrobroom version;
2. the command or TrenchBroom profile you used;
3. whether the problem happens with `--no-lightmaps`;
4. the smallest `.map` or NIF that reproduces it;
5. the compiler's output, and OpenMW's log if it got that far.

[Open a GitHub issue](https://github.com/DreamWeave-MP/Morrobroom/issues) with that information,
or ask in [Discussions](https://github.com/DreamWeave-MP/Morrobroom/discussions) if you are not sure
it is a bug. A small reproducible crater is vastly more useful than a large screenshot of one.
