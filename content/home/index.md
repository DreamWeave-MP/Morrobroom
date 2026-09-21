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
