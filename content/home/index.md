+++
title = "Morrobroom"
description = "Build Morrowind levels in TrenchBroom: a brush compiler, lightmapper and NIF-to-TrenchBroom importer for OpenMW."

[taxonomies]
tags = ["Morrowind", "OpenMW", "TrenchBroom", "Rust"]
+++

Morrobroom lets you build Morrowind and OpenMW spaces in
[TrenchBroom](https://trenchbroom.github.io/), then compile them into native game content:
meshes, a plugin, and baked lighting.

Build rooms, corridors, structures and blockouts from brushes instead of assembling every wall
from prebuilt meshes. Build lights, containers, activators and items the same way. And when
the geometry you want already exists as a NIF, `nif2map` turns it back into brushes you can
edit.

{{ schematic(data_path="data/schematics/workflow.json") }}

## Start here

The [manual](@/docs/_index.md) is small and practical. Begin with
[Start here](@/docs/start-here.md) if this is your first time using TrenchBroom with OpenMW:
it takes you from nothing installed to walking around your first room.

You do not need to know Quake mapping, NIF internals, CSG or Rust. You need TrenchBroom, OpenMW
with Morrowind's data, and the Morrobroom download for your platform, above.

If you have never used TrenchBroom, start with
[DumptruckDS's TrenchBroom playlist](https://youtube.com/playlist?list=PLgDKRPte5Y0AZ_K_PZbWbgBAEt5xf74aE&si=0HrERzygljsiMz2h)
and keep the [official TrenchBroom manual](https://trenchbroom.github.io/manual/latest/) nearby.
For the story behind all of this, read OpenMW's
[From BSP to ESP](https://openmw.org/2024/from-bsp-to-esp-how-s3ctor-abused-quake-editors-to-redefine-the-morrowind-modding-experience/).

## What it does

- Compiles Valve 220 `.map` files into NIF meshes and a Morrowind or OpenMW plugin, with
  statics, activators, containers, items, lights and leveled lists built from what you placed.
- Bakes colored static lighting into BC7 lightmaps, which OpenMW loads without an engine patch.
- Carries NIF materials, transparency, texturing, scrolling and swaying UVs, billboards, sort nodes
  and collision roots from TrenchBroom into the meshes it writes.
- Generates a TrenchBroom entity catalog from an OpenMW load order, and places the records you
  pick from it.
- Reverse-compiles visual NIF geometry into editable maps with `nif2map`.

## Status

Morrobroom is usable and tested, but unusual brushes, exotic NIFs and strange third-party assets
can still find edge cases. Small reproducible examples are extremely useful:
[Troubleshooting](@/docs/troubleshooting.md#reporting-a-problem) says what to send.

Morrobroom is AGPL-3.0. The code under `src/slipgate/` is MIT or Apache-2.0; see
[`THIRD_PARTY.md`](https://github.com/DreamWeave-MP/Morrobroom/blob/main/THIRD_PARTY.md).
