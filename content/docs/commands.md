+++
title = "Tools and Command Line"
description = "Use Morrobroom directly when TrenchBroom's compile profile is not enough."
weight = 40

[extra]
kind = "reference"
+++

## Compile a map

The normal command is:

```bash
morrobroom compile \
  --map my_level.map \
  --output my_level.omwaddon
```

Lightmapping is enabled by default. To keep generated meshes, lightmaps, and
the plugin in one project-local directory:

```bash
morrobroom compile \
  --map my_level.map \
  --output-dir build/my_level \
  --output build/my_level/my_level.omwaddon
```

Useful compile options:

| Option | Purpose |
| --- | --- |
| `--map PATH` | Required input `.map` file. |
| `--output PATH` | Plugin output: `.esp`, `.esm`, `.omwaddon`, or `.omwgame`. |
| `--output-dir PATH` | Root for generated meshes and lightmaps. |
| `--scale NUMBER` | Scale generated meshes; defaults to `1.0`. |
| `--no-lightmaps` | Disable lightmap UVs and baking. |

The input map must exist and use the `.map` extension. Output directories are
created when necessary.

## Generate an FGD

```bash
morrobroom FGD \
  --config /path/to/openmw.cfg
```

The catalog is written to TrenchBroom's standard per-user Morrowind directory
and includes `Morrowind.fgd`. Use `--output <path>` to override the location.
See [OpenMW integration](@/docs/openmw.md#regenerate-the-fgd) for details.

## Import NIF geometry

See the dedicated [NIF Import](@/docs/nif-import.md) page for the structural
versus surface reconstruction model, output files, brush limits, and import
options. The short command is:

```bash
morrobroom nif2map meshes/ \
  --recursive \
  --texture-root "/path/to/Morrowind/Data Files" \
  --texture-root "/path/to/Morrowind/Data Files/Morrowind.bsa" \
  --output-dir nif2map-out
```

Run `morrobroom --help` or a subcommand's `--help` for the complete option
list.
