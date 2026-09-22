+++
title = "Mapping"
description = "The TrenchBroom concepts and authoring rules Morrobroom expects."
weight = 20

[extra]
kind = "workflow"
+++

## The small vocabulary

- A **brush** is an editable convex solid. Rooms, floors, walls, ceilings, and
  most architectural blockouts begin as brushes.
- A **face** is one surface of a brush. Faces carry texture projections and
  surface attributes.
- An **entity** is an object described by properties. Lights, doors, statics,
  and items are entities rather than brush solids.
- An **FGD** is the entity definition file that tells TrenchBroom which objects
  exist and which properties they accept.
- **Compiling** converts the saved `.map` into generated NIF geometry and a
  TES3/OpenMW plugin.

Morrobroom uses **Quake 2 maps with Valve 220 texture projection**. That is
the format selected by the supplied Morrowind game configuration. TrenchBroom
is doing the editing; Morrobroom is responsible for turning the resulting
brushes into Morrowind assets.

## A useful authoring loop

1. Block out the room with simple brushes.
2. Check that every room is enclosed and every brush is a valid solid.
3. Apply textures and add entities.
4. Save the map.
5. Compile early, then make the next change.

Compiling a small test map is more informative than building an entire dungeon
before discovering that one brush has been quietly shaped like a legal theory.

## Materials and surface tags

The supplied game configuration searches the Morrowind `Data Files/` directory
for textures. It recognizes `.tga`, `.png`, `.dds`, and `.webp` files. The
special `skip` and `clip` materials are transparent editor helpers:

- `skip` marks a face that should not become visible geometry;
- `clip` marks a face intended for collision-only behavior.

Use ordinary Morrowind materials for faces you want rendered. If a texture is
missing in TrenchBroom, fix the game directory or texture path before blaming
the compiler. The compiler is many things, but it is not a texture diviner.

## Entities

Use the FGD-backed entity browser to place lights and Morrowind records. A
point light such as `Light_Point1024` is a good first test because it makes the
compiled room's lighting obvious. Static objects, doors, activators, items,
and other supported records are compiled into actual game references.

The base FGD is useful for getting started. If your installed game or mods add
records that are not present, [regenerate the FGD](@/docs/openmw.md#regenerate-the-fgd).

## Imported geometry

For the full NIF workflow, see [NIF import](@/docs/nif-import.md). The short
version is that `nif2map` is a general visual-geometry reconstruction path, not
just an architectural-asset utility.
