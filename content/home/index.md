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

**TrenchBroom is the editor. Morrobroom is the compiler. OpenMW is where you
play the result.**

Download the build for your platform above, or
[browse the source on GitHub](https://github.com/DreamWeave-MP/Morrobroom).

<!-- more -->

## What you get

- Valve 220 `.map` files compiled into NIF geometry and Morrowind/OpenMW plugins.
- Baked colored lighting with BC7-compressed lightmaps.
- `nif2map` for bringing compatible NIF geometry into TrenchBroom.
- Generated FGD data so TrenchBroom can place objects from your actual OpenMW setup.

## New to TrenchBroom?

You do **not** need to know Quake mapping, NIF internals, CSG, or Rust to use
Morrobroom.

A few words will get you surprisingly far:

- **Brush** — a simple editable 3D solid. Walls, floors, ceilings, stairs, and
  most architecture start here.
- **Face** — one surface of a brush. Faces are where textures are applied.
- **Entity** — a game or editor object, such as a light, door, weapon, or static.
- **FGD** — the definitions that tell TrenchBroom which Morrowind entities exist.
- **Compile** — turn the `.map` into NIFs, a plugin, and any generated textures.

If you have never used TrenchBroom before, start with
[DumptruckDS's TrenchBroom playlist](https://youtube.com/playlist?list=PLgDKRPte5Y0AZ_K_PZbWbgBAEt5xf74aE&si=0HrERzygljsiMz2h).
It is a much better introduction to actually moving around the editor and
building with brushes than this page needs to become.

The full [TrenchBroom manual](https://trenchbroom.github.io/manual/latest/) is
also excellent once you want to go deeper.

## Set up TrenchBroom

Morrobroom ships with a TrenchBroom game configuration under `resources/`.

Copy those files into TrenchBroom's custom `Morrowind` game directory:

- Windows: `%APPDATA%\TrenchBroom\games\Morrowind`
- macOS: `~/Library/Application Support/TrenchBroom/games/Morrowind`
- Linux: `~/.TrenchBroom/games/Morrowind`

Start TrenchBroom. If **Morrowind** does not appear in the game list, stop here:
the game configuration is not installed in the right place yet.

Open **Preferences → Games → Morrowind**.

Set the game directory to the folder that **contains** `Data Files/`, not
`Data Files/` itself.

For example:

```text
Morrowind/
└── Data Files/
```

Then configure the compilation tools:

- `Morrobroom` — the Morrobroom executable
- `OpenMW` — the OpenMW executable
- `OpenCS` — optional

The supplied **Map-to-Engine** profile will compile the current map and launch
it directly in OpenMW.

**Save your map before compiling.** Morrobroom compiles the saved `.map` file.

## Your first room

The goal here is simple: make a room, put a light in it, and stand inside it in
OpenMW.

1. Create a new **Morrowind** map in TrenchBroom.
2. Build a small enclosed room out of brushes.
3. Apply any Morrowind texture to the visible faces.
4. Place a `Light_Point1024` entity inside the room.
5. Save the map.
6. Open TrenchBroom's compile dialog and run **Map-to-Engine**.

Morrobroom will create the compiled plugin, generated meshes, and lightmaps
under the map's local `build/` directory, then launch OpenMW.

If OpenMW launches into your room, you are done. Build something less boring.

If it launches but you see an old version of the map, save the `.map` and
compile again.

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

If you add or remove content from your OpenMW configuration later, regenerate
the FGD to keep TrenchBroom in sync.

## Compile from the command line

You do not need the command line for the normal TrenchBroom workflow, but the
same compiler can be used directly.

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

For most maps, just place lights in TrenchBroom and compile. You do not need to
manually create UVs or lightmap textures.

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
