+++
title = "OpenMW Integration"
description = "FGDs, game objects, generated output, and baked static lighting."
weight = 30

[extra]
kind = "reference"
+++

## What a compile produces

Morrobroom turns the saved map into:

- generated NIF meshes for brush geometry;
- a TES3-compatible plugin (`.esp`, `.esm`, `.omwaddon`, or `.omwgame`);
- generated textures when lightmapping is enabled;
- references for supported Morrowind entities placed in TrenchBroom.

The TrenchBroom **Map-to-Engine** profile keeps these files together under
`build/<map-name>/`. That keeps separate maps from overwriting each other's
meshes and avoids writing generated content into the global OpenMW data
directory.

## Regenerate the FGD

The FGD is for TrenchBroom. It is not an OpenMW data file. Generate one from
the active OpenMW configuration when the included definitions do not match your
installed game and mods:

```bash
morrobroom FGD \
  --config /path/to/openmw.cfg \
  --output /path/to/TrenchBroom/games/Morrowind/Morrowind.fgd
```

The destination is the custom TrenchBroom `Morrowind` directory from
[Start Here](@/docs/start-here.md#install-the-trenchbroom-game-files). On
Windows, macOS, and Linux, the generated file belongs beside `GameConfig.cfg`,
not in `Data Files/`.

To generate only selected placeable record types, use the TES3 four-letter
tags:

```bash
morrobroom FGD \
  --config /path/to/openmw.cfg \
  --types "STAT;DOOR;LIGH;ACTI" \
  --output /path/to/TrenchBroom/games/Morrowind/Morrowind.fgd
```

Regenerate after changing the OpenMW configuration if new records need to
appear in TrenchBroom.

## Baked lighting

Lighting is enabled by default. Morrobroom creates a second UV set for static
geometry, bakes ambient light plus authored static lights and visibility, and
writes the result as a BC7 DDS lightmap attached through Morrowind's existing
`DarkTexture` path.

This works in OpenMW without an engine patch. It is a static bake, not global
illumination: moving actors and other dynamic objects still use normal OpenMW
lights. Disable the bake with `--no-lightmaps` when iterating on geometry or
when a map does not need it.

## Scale and plugins

Morrowind assets and Quake-style maps do not always use the same practical
scale. The compiler defaults to `1.0`; use `--scale` when importing maps
authored at another scale. The TrenchBroom profile uses `1.0`.

The output plugin can be an `.esp`, `.esm`, `.omwaddon`, or `.omwgame`. For
normal TrenchBroom iteration, `.omwaddon` in the project-local build directory
is the least surprising choice.
