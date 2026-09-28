+++
title = "OpenMW integration"
description = "What a compile writes, how plugins are updated, baked lighting, scale, and the entity catalog for your load order."
weight = 35

[extra]
kind = "reference"
+++

## What a compile writes

`morrobroom compile` turns the saved map into:

| Output | Where |
| --- | --- |
| One NIF per mesh: worldspawn, each group, each brush entity | `Meshes/<map name>/` under the output directory |
| The baked lightmap, unless `--no-lightmaps` | `Textures/<map name>/lightmap.dds` |
| The plugin: a cell, its references, and a record for everything you built | `--output`, by default `<map name>.omwaddon` beside the map |

The output directory is `--output-dir`, and by default the folder the map is in. The TrenchBroom
**Map-to-Engine** profile points both at `build/<map name>/`, which keeps each map's files
together, keeps maps from overwriting each other's meshes, and keeps generated content out of your
real data directories. That folder is a complete data directory: OpenMW loads it with `--data`.

To find textures, Morrobroom reads your OpenMW configuration the way OpenMW does, starting from
`openmw-config`'s root discovery and falling back to the user's config. Pass `--config` with a
config file or directory to compile against a different loadout.

## Compiling into an existing plugin

If the plugin already exists, Morrobroom updates it: the records this map made last time,
including its cell, are replaced, and everything else in the plugin stays. Dialogue, scripts and
other records you add in OpenCS survive a recompile. Changes made in OpenCS to the compiled cell or
to the records the map made do not: those belong to the map.

The Map-to-Engine profile deletes the plugin before compiling, so every test run starts clean.
Turn that step off in **Run → Compile Map** if you are building on top of a plugin.

## Baked lighting

Lighting is on by default. Morrobroom creates a second UV set for static geometry, bakes ambient
light plus the map's point lights and their shadows, and writes the result as a BC7 DDS lightmap
attached through Morrowind's existing dark map slot.

OpenMW loads it without an engine patch. It is a static bake, not global illumination: actors and
anything else that moves are still lit by OpenMW's ordinary lights, which the compiled light
records provide. Turn the bake off with `--no-lightmaps` while iterating on geometry, or when a map
does not need it.

An entity with its own `Nif_Texture_DarkMap` keeps it, instead of the lightmap. See
[NIF authoring](@/docs/nif-authoring.md#texturing).

## Scale

Quake-style maps and Morrowind assets do not share a scale: Quake maps come out about half the size
of Morrowind's. The compiler scales by `2.0` by default, for Morrowind-sized content, and the
Map-to-Engine profile passes the same. Use `--scale` for maps
authored at another scale, and keep the same value for `fgd --scale`, so catalog entities are the
size they will be in game.

## Plugin types

The plugin can be `.esp`, `.esm`, `.omwaddon` or `.omwgame`; the extension decides. For
TrenchBroom iteration, an `.omwaddon` in the map's build directory is the least surprising choice.

## The entity catalog for your load order

`Morrowind.fgd` is the compact schema for editing. To see your installed game's and mods' records
in TrenchBroom's entity browser, with their models, generate a catalog from your OpenMW
configuration:

```bash
morrobroom fgd --config /path/to/openmw.cfg
```

It writes `MorrowindObjects.fgd` into TrenchBroom's per-user `Morrowind` game directory from
[Start here](@/docs/start-here.md#2-install-the-trenchbroom-game-files), beside `Morrowind.fgd` and
`GameConfig.cfg`, never into `Data Files/`. It refuses when Morrobroom's game files are not
installed there yet. Use `--output <path>` for TrenchBroom's portable mode, or anywhere else.

To catalog only some record types, list their TES3 tags:

```bash
morrobroom fgd --config /path/to/openmw.cfg --types "stat;door;ligh;acti"
```

Regenerate after changing your load order, then reload the game configuration in TrenchBroom.

The catalog is for browsing and judging scale: placing its entries does not compile into references
yet. [Entities](@/docs/entities.md#not-compiled-yet) says what does.
