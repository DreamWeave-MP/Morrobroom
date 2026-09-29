+++
title = "Command line"
description = "Every Morrobroom command and option, for when TrenchBroom's compile profile is not enough."
weight = 45

[extra]
kind = "reference"
+++

Morrobroom has three commands. `morrobroom --help`, or `--help` after any command, prints the
same options from the program itself.

| Command | Does |
| --- | --- |
| [`compile`](#compile) | Compile a TrenchBroom map into meshes, a lightmap and a plugin. |
| [`nif2map`](#nif2map) | Reverse-compile NIF visual geometry into editable maps. |
| [`fgd`](#fgd) | Generate a TrenchBroom entity catalog from an OpenMW load order. |

## compile

```bash
morrobroom compile --map my_level.map --output my_level.omwaddon
```

To keep the generated meshes, lightmap and plugin together in one project-local directory, the way
the TrenchBroom profile does:

```bash
morrobroom compile \
  --map my_level.map \
  --output-dir build/my_level \
  --output build/my_level/my_level.omwaddon
```

| Option | Default | Purpose |
| --- | --- | --- |
| `--map PATH` | | **Required.** The `.map` file to compile. It must exist and end in `.map`. |
| `-o`, `--output PATH` | `<map>.omwaddon` beside the map | The plugin: `.esp`, `.esm`, `.omwaddon` or `.omwgame`. An existing plugin is updated, not replaced; see [OpenMW integration](@/docs/openmw.md#compiling-into-an-existing-plugin). |
| `--output-dir PATH` | the map's folder | Where `Meshes/` and `Textures/` are written. Created if needed. |
| `-c`, `--config PATH` | `openmw-config` discovery, then the user config | The OpenMW config file or directory whose data provides textures. |
| `-s`, `--scale NUMBER` | `2.0` | Scale the generated meshes. Quake-scale maps need `2.0` to match Morrowind. |
| `--no-lightmaps` | | Skip lightmap UVs and baking. |

## nif2map

```bash
morrobroom nif2map meshes/ \
  --recursive \
  --texture-root "/path/to/Morrowind/Data Files" \
  --texture-root "/path/to/Morrowind/Data Files/Morrowind.bsa" \
  --output-dir nif2map-out
```

It writes map units, dividing the NIF's Morrowind units by `-s`, `--scale` (`2.0`, the same as
`compile`). Every option, and the model behind them, is on
[NIF import](@/docs/nif-import.md#options-that-matter).

## fgd

```bash
morrobroom fgd --config /path/to/openmw.cfg
```

| Option | Default | Purpose |
| --- | --- | --- |
| `-c`, `--config PATH` | | The `openmw.cfg` whose load order to catalog. |
| `-o`, `--output PATH` | TrenchBroom's `Morrowind` game directory | Where to write `MorrowindObjects.fgd`. |
| `-t`, `--types TAGS` | every placeable type | TES3 record tags separated by `;`, like `"stat;door;ligh"`. Placeable objects only: `acti`, `alch`, `appa`, `armo`, `book`, `clot`, `door`, `ingr`, `levc`, `levi`, `ligh`, `lock`, `misc`, `prob`, `repa`, `scpt`, `stat`, `weap`. |
| `-s`, `--scale NUMBER` | `2.0` | Scale for the catalog's bounding boxes. Keep it the same as `compile --scale`. |

[OpenMW integration](@/docs/openmw.md#the-entity-catalog-for-your-load-order) explains where the
catalog goes and what it is for.
