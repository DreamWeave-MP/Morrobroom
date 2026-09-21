+++
title = "Morrobroom"
description = "A Rust-powered Morrowind and OpenMW brush compiler, lightmapper, and NIF-to-TrenchBroom importer."
date = 2026-09-21

[taxonomies]
tags = ["Morrowind", "OpenMW", "TrenchBroom", "Rust"]

[extra]
hide_download_bar = false
use_toc = false
is_binary = true
game = "morrowind"
binary_name = "morrobroom"
version = "1.0.0"
stable_title = "Download the latest Morrobroom release"
dev_title = "Download the latest Morrobroom development build"
+++

# Build better spaces for OpenMW

Morrobroom turns TrenchBroom brush maps into native Morrowind and OpenMW
content. It brings a fast, editable brush workflow to a game that normally
expects every wall, floor, and object to arrive as a prebuilt mesh.

Download the build for your platform above, or [browse the source on
GitHub](https://github.com/DreamWeave-MP/Morrobroom).

<!-- more -->

## What it does

- Compiles Valve 220 `.map` files into `.omwaddon`, `.esp`, `.esm`, and related
  OpenMW content.
- Generates NIF meshes and object records from brush geometry.
- Bakes static lighting into BC7 lightmaps while preserving runtime lighting
  for actors and other dynamic objects.
- Imports compatible Morrowind NIF geometry back into editable TrenchBroom
  maps with `nif2map`.
- Produces FGD data for Morrowind object types and editor workflows.

## Quick start

Morrobroom requires an OpenMW installation with a working `openmw.cfg` and
access to your Morrowind data files.

Compile a map with lightmapping enabled:

```bash
morrobroom compile \
  --map my_level.map \
  --output my_level.omwaddon
```

Disable baked lightmaps when you want OpenMW to handle lighting at runtime:

```bash
morrobroom compile \
  --map my_level.map \
  --output my_level.omwaddon \
  --no-lightmaps
```

Import NIF geometry into a map:

```bash
morrobroom nif2map meshes/ \
  --recursive \
  --output-dir nif2map-out
```

After compiling, add the generated data directory and plugin to OpenMW in the
usual way. The release archive contains the executable for your platform; it
does not include Morrowind's game data.

## TrenchBroom setup

Copy the contents of the repository's `resources/` directory into the custom
`Morrowind` game directory for TrenchBroom:

- Windows: `%APPDATA%\TrenchBroom\games\Morrowind`
- macOS: `~/Library/Application Support/TrenchBroom/games/Morrowind`
- Linux: `~/.TrenchBroom/games/Morrowind`

In **Preferences → Games → Morrowind**, set the Morrowind game directory to
the directory containing `Data Files/` — not `Data Files/` itself — and
configure the compilation tools:

- `Morrobroom` — the downloaded Morrobroom executable
- `OpenMW` — the OpenMW executable used to launch compiled maps
- `OpenCS` — optional, for finishing plugins in OpenMW Construction Set

The supplied **Map-to-Engine** profile uses those tool names and stages each
map under its own `build/<map-name>/` directory. It does not assume
`/usr/bin/openmw` or write generated assets into the global OpenMW data
directory. Configure an OpenMW engine profile separately if you want
TrenchBroom's normal **Launch** command as well.

**Save your map before compiling.** This profile compiles the saved `.map`
file directly rather than creating a temporary export through TrenchBroom.
The on-disk map is the source of truth.

If the included object definitions do not match the installed game data,
regenerate `Morrowind.fgd` from the active `openmw.cfg` and write it into the
custom game directory listed above. For example:

```bash
morrobroom FGD \
  --config /path/to/openmw.cfg \
  --output /path/to/TrenchBroom/games/Morrowind/Morrowind.fgd
```

The destination is:

- Windows: `%APPDATA%\TrenchBroom\games\Morrowind\Morrowind.fgd`
- macOS: `~/Library/Application Support/TrenchBroom/games/Morrowind/Morrowind.fgd`
- Linux: `~/.TrenchBroom/games/Morrowind/Morrowind.fgd`

## Lightmapping

Static geometry uses UV set 0 for its base texture and UV set 1 for its baked
lightmap. The compiler writes power-of-two BC7 DDS atlases for the OpenMW
texture path, with authored ambient color and direct point-light visibility
baked into the result. Runtime lights remain available for actors and other
dynamic objects.

## Project status

Morrobroom is usable software, but it is still an active tool. The geometry,
CSG, lightmapping, NIF import, and FGD paths have substantial automated test
coverage. Unusual brush geometry, exotic NIF structures, and third-party asset
assumptions may still require a small amount of patience.

Small reproducible bug reports are welcome in the
[GitHub issue tracker](https://github.com/DreamWeave-MP/Morrobroom/issues).

## License and credits

Morrobroom is open source. See the repository's license and third-party
notices for the complete attribution list.

It is built with [OpenMW](https://openmw.org/),
[TrenchBroom](https://trenchbroom.github.io/), and the Rust ecosystem,
including [`tes3`](https://github.com/Greatness7/tes3),
[`openmw-config`](https://github.com/DreamWeave-MP/Openmw_Config), and
[`vfstool_lib`](https://github.com/DreamWeave-MP/vfstool).
