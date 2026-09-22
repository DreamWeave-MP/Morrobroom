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

**TrenchBroom is the editor. Morrobroom is the compiler. OpenMW is where you
play the result.**

Build rooms, corridors, structures, and blockouts with brushes instead of
assembling every wall from prebuilt meshes. Morrobroom can bake static
lighting, place supported Morrowind objects, and reverse-compile visual NIF
geometry into editable `.map` form.

Download the build for your platform above, or
[browse the source on GitHub](https://github.com/DreamWeave-MP/Morrobroom).

<!-- more -->

## Start here

The [Morrobroom manual](@/docs/_index.md) is a small, practical guide for the
whole workflow. Begin with [Start Here](@/docs/start-here.md) if this is your
first time using TrenchBroom with OpenMW.

You do not need to know Quake mapping, NIF internals, CSG, or Rust to begin.
You need TrenchBroom, OpenMW with Morrowind data available, and a Morrobroom
release. The manual covers setup, your first room, FGDs, lighting, command-line
use, and troubleshooting without making the homepage serve as a second manual.

For the story behind the workflow, read OpenMW's
[From BSP to ESP](https://openmw.org/2024/from-bsp-to-esp-how-s3ctor-abused-quake-editors-to-redefine-the-morrowind-modding-experience/).

If you have never used TrenchBroom before, start with
[DumptruckDS's TrenchBroom playlist](https://youtube.com/playlist?list=PLgDKRPte5Y0AZ_K_PZbWbgBAEt5xf74aE&si=0HrERzygljsiMz2h),
then keep the [official TrenchBroom manual](https://trenchbroom.github.io/manual/latest/)
nearby.

## What it does

- Compiles Valve 220 `.map` files into NIF geometry and
  Morrowind/OpenMW plugins.
- Bakes colored static lighting into BC7 lightmaps.
- Generates FGD definitions from an OpenMW configuration.
- Reverse-compiles visual NIF geometry with `nif2map`.

## Status

Morrobroom is usable and tested, but unusual brushes, exotic NIFs, and strange
third-party assets can still expose edge cases. Small reproducible examples are
extremely useful. Report problems on the
[GitHub issue tracker](https://github.com/DreamWeave-MP/Morrobroom/issues).

Morrobroom is open source. See the repository license and
[`THIRD_PARTY.md`](https://github.com/DreamWeave-MP/Morrobroom/blob/main/THIRD_PARTY.md)
for attribution.
