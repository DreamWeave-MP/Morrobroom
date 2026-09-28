+++
title = "Mapping"
description = "The TrenchBroom vocabulary and the authoring rules Morrobroom expects."
weight = 20

[extra]
kind = "guide"
+++

## The small vocabulary

- A **brush** is an editable convex solid. Rooms, floors, walls, ceilings and most architectural
  blockouts begin as brushes.
- A **face** is one surface of a brush. Faces carry a texture, its projection, and surface flags.
- An **entity** is an object described by properties. Lights, doors, containers and items are
  entities. Some entities are points; some own brushes.
- An **FGD** is the entity definition file that tells TrenchBroom which entities exist and which
  properties they accept.
- **Compiling** turns the saved `.map` into NIF meshes, lightmaps and a TES3 plugin.

Morrobroom reads **Quake 2 maps with Valve 220 texture projection**, the format the supplied
Morrowind game configuration selects. TrenchBroom does the editing; Morrobroom is responsible for
turning the resulting brushes into Morrowind assets.

## How a map becomes game content

| In TrenchBroom | In the plugin |
| --- | --- |
| The map | One interior cell, named after the map unless worldspawn says otherwise |
| Brushes that belong to no entity (worldspawn) | One static mesh, placed in the cell |
| A group or layer of brushes | Its own static mesh, placed in the cell |
| A brush entity (`world_Detail`, `item_Misc`, …) | Its own mesh, a record of that type, and a reference to it |
| A light | A light record and a reference, and light in the baked lightmap |

[Entities](@/docs/entities.md) lists every entity and what it becomes.

## A useful authoring loop

1. Block out the room with simple brushes.
2. Check that every room is enclosed and every brush is a valid solid.
3. Apply textures and add entities.
4. Save the map.
5. Compile early, then make the next change.

Compiling a small test map is more informative than building an entire dungeon before discovering
that one brush has been quietly shaped like a legal theory.

## Textures

The game configuration looks for textures in Morrowind's `Data Files/textures`, and recognizes
`.tga`, `.png`, `.dds` and `.webp` files. Normal, specular and similar maps (`*_n`, `*_nh`,
`*_spec`) are hidden from the texture browser, because nobody paints a wall with a normal map on
purpose.

If a texture is missing in TrenchBroom, fix the game directory or the texture path before blaming
the compiler. The compiler is many things, but it is not a texture diviner.

## Faces that do something special

| Texture or flag | Effect |
| --- | --- |
| `skip`, or any texture with `skip_` in its name | The face is not built: no mesh, no collision. For faces nobody will ever see. |
| A texture with `water`, `slime`, `lava` or `mwat` in its name | Drawn, but never collides, so the player can wade into it. |
| **NoClip** surface flag | Drawn, but never collides. |
| **Invert Faces** surface flag | Drawn inside out, for fake skyboxes and other things seen from within. |
| **Smooth Shading** surface flag | Not implemented yet; faces are flat shaded. |

`clip` is drawn transparent in TrenchBroom, but the compiler builds it like any other face. It is
not an invisible collision material.

## Groups and layers

A TrenchBroom group or layer is also a unit of compilation: its brushes become their own mesh and
their own placed object, not part of the map's main mesh. Group things that belong together, like
a pillar with its base, and keep the room's shell in worldspawn.

Groups also scope NIF structure: billboard, sort and collision nodes placed in a group apply to that
group's geometry. [NIF authoring](@/docs/nif-authoring.md#structure-groups-and-links) explains how.

## Imported geometry

To start from geometry that already exists as a NIF, see [NIF import](@/docs/nif-import.md). The
short version: `nif2map` turns a mesh's visual geometry back into brushes, which you then edit like
anything else.
