+++
title = "Troubleshooting"
description = "A short diagnostic guide for the problems you are most likely to meet."
weight = 50

[extra]
kind = "diagnostics"
+++

## TrenchBroom does not show Morrowind

Check the custom game directory first. It must contain `GameConfig.cfg`,
`CompilationProfiles.cfg`, and the editor resources copied from Morrobroom's
`resources/` directory. Restart TrenchBroom after copying them.

Then check the game directory setting. It must point to the folder containing
`Data Files/`, not to `Data Files/` itself.

## Objects are missing from the entity browser

The FGD is a TrenchBroom definition file. If an object from an installed mod
does not appear, generate `MorrowindObjects.fgd` from the active `openmw.cfg`
with `morrobroom FGD`. It is written to the standard TrenchBroom user-game
directory and includes the handwritten `Morrowind.fgd`. Restart or reload the
game configuration after generation. Use `--output <path>` for portable mode.

## OpenMW launches the old map

Save the `.map` before compiling. The compiler reads the saved file, not
TrenchBroom's unsaved editor state. Also check that the compile profile is
pointing OpenMW at `build/<map-name>/` and launching the matching
`<map-name>.omwaddon`.

## The map compiles but textures are missing

Confirm that the configured Morrowind game directory contains the expected
`Data Files/` and that the texture exists under its `textures/` path. The
supplied configuration recognizes `.tga`, `.png`, `.dds`, and `.webp` files.
Missing textures in the editor are usually a TrenchBroom search-path problem,
not a plugin problem.

## Geometry fails or looks wrong

Start with a smaller map and isolate the newest brush or entity. Check for
malformed or zero-volume solids, degenerate faces, extremely thin slivers, and
geometry that was imported from a NIF rather than authored as a brush. Compile
early and often; the compiler has no obligation to conceal a cursed polygon
until the end of the project.

For imported assets, use `nif2map --dry-run --verbose` first. Check the
`.nif2map.json` report and watch the generated brush count; complex visual
geometry can produce very large `.map` files. See [NIF import](@/docs/nif-import.md)
for the structural and surface reconstruction model.

## Lighting is absent or stale

Make sure the map contains a supported static light entity and that the compile
was not run with `--no-lightmaps`. Delete or replace the map's local `build/`
directory if old generated textures are making the result difficult to read,
then compile again.

## Reporting a problem

Include:

1. operating system and Morrobroom version;
2. the command or TrenchBroom profile used;
3. whether the problem occurs with `--no-lightmaps`;
4. the smallest `.map` or NIF that reproduces it;
5. the relevant compiler or OpenMW log.

[Open a GitHub issue](https://github.com/DreamWeave-MP/Morrobroom/issues) with
that information. A small reproducible crater is vastly more useful than a
large screenshot of one.
