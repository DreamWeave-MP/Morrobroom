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

# Build Morrowind levels in TrenchBroom

Morrobroom lets you build Morrowind and OpenMW spaces with
[TrenchBroom](https://trenchbroom.github.io/), then compile them into native
game content.

Build rooms, corridors, structures, or entire levels with brushes instead of
assembling every wall from prebuilt meshes. Morrobroom can also bake static
lighting, place real Morrowind objects, and turn compatible NIFs back into
editable `.map` geometry.

Download the build for your platform above, or
[browse the source on GitHub](https://github.com/DreamWeave-MP/Morrobroom).

<!-- more -->

## What you get

- Valve 220 `.map` files compiled into NIF geometry and Morrowind/OpenMW plugins.
- Baked colored lighting with BC7-compressed lightmaps.
- `nif2map` for bringing compatible NIF geometry into TrenchBroom.
- Generated FGD data so TrenchBroom can place objects from your actual OpenMW setup.

## Set up TrenchBroom

Morrobroom ships with a TrenchBroom game configuration under `resources/`.

Copy those files into TrenchBroom's custom `Morrowind` game directory:

- Windows: `%APPDATA%\TrenchBroom\games\Morrowind`
- macOS: `~/Library/Application Support/TrenchBroom/games/Morrowind`
- Linux: `~/.TrenchBroom/games/Morrowind`

Then open **Preferences → Games → Morrowind**.

Set the game directory to the folder that **contains** `Data Files/`, then set:

- `Morrobroom` — the Morrobroom executable
- `OpenMW` — the OpenMW executable
- `OpenCS` — optional

The supplied **Map-to-Engine** profile will compile the current map and launch
it in OpenMW.

**Save your map before compiling.** Morrobroom compiles the saved `.map` file.

## Generate object definitions

The included FGD gives TrenchBroom its Morrowind entities. If it does not match
your installed game and mods, regenerate it from your active `openmw.cfg`:

```bash
morrobroom FGD \
  --config /path/to/openmw.cfg \
  --output /path/to/TrenchBroom/games/Morrowind/Morrowind.fgd
```

This is what lets TrenchBroom know about the objects available in your actual
OpenMW setup.

## Compile from the command line

Lightmapping is enabled by default:

```bash
morrobroom compile \
  --map my_level.map \
  --output my_level.omwaddon
```

Disable baked lightmaps with:

```bash
morrobroom compile \
  --map my_level.map \
  --output my_level.omwaddon \
  --no-lightmaps
```

Import NIF geometry with:

```bash
morrobroom nif2map meshes/ \
  --recursive \
  --output-dir nif2map-out
```

## Lightmapping

Morrobroom can bake colored static lighting and shadows directly into generated
geometry. The result is stored as a BC7 lightmap and rendered through OpenMW's
existing texture pipeline.

Static architecture uses the bake. Actors and other dynamic objects can still
use normal OpenMW lights.

## Status

Morrobroom is usable, tested, and still capable of finding creative new ways
to explode when fed sufficiently cursed geometry.

Unusual brushes, exotic NIFs, and strange third-party assets may still expose
edge cases. Small reproducible examples are extremely useful.

Report problems on the
[GitHub issue tracker](https://github.com/DreamWeave-MP/Morrobroom/issues).

## License and credits

Morrobroom is open source. See the repository license and third-party notices
for full attribution.

Built with [OpenMW](https://openmw.org/),
[TrenchBroom](https://trenchbroom.github.io/),
[`tes3`](https://github.com/Greatness7/tes3),
[`openmw-config`](https://github.com/DreamWeave-MP/Openmw_Config), and
[`vfstool_lib`](https://github.com/DreamWeave-MP/vfstool).
