+++
title = "NIF authoring"
description = "Set materials, transparency, texturing, moving textures and NIF structure from TrenchBroom."
weight = 30

[extra]
kind = "reference"
+++

Every brush entity compiles into its own NIF, and its properties decide what goes into that NIF
beyond the geometry: how it is lit, how it blends, how its textures are sampled and animated, and
how its nodes are arranged. Worldspawn takes them too, for the map's main mesh.

Everything here is optional. Leave a property unset and the compiler uses the NIF default, which
is what an ordinary wall wants. The definitions live in `Nif.fgd`.

For geometry that needs these properties but no game record of its own, use `nif_geometry`: it
compiles to a plain static, like `world_Detail`.

## Material

Applied to the whole entity.

| Property | Effect |
| --- | --- |
| `Material_Diffuse_color` | The surface color under light. |
| `Material_Ambient_color` | The surface color under ambient light. |
| `Material_Emissive_color` | Light the surface gives off itself: glowing runes, lava, a lit window. |
| `Material_Specular_color`, `Material_Glossiness` | Highlights, and how tight they are. |
| `Material_Alpha` | Opacity, from `0` (invisible) to `1`. Needs blending, below, to show. |

## Transparency

Blending mixes the surface with what is behind it; testing cuts pixels out entirely. Use blending
for glass and smoke, testing for leaves, grates and anything with hard holes.

| Property | Effect |
| --- | --- |
| `Material_Alpha_UseBlend` | Turn alpha blending on. |
| `Material_Alpha_BlendSourceMode`, `Material_Alpha_BlendDestinationMode` | The blend equation. Source Alpha with One Minus Source Alpha is ordinary transparency; Source Alpha with One is additive glow. |
| `Material_Alpha_TestEnable` | Turn alpha testing on. |
| `Material_Alpha_TestFunction`, `Material_Alpha_TestThreshold` | Which pixels survive the test. The threshold is `0` to `255`; anything else is ignored and the default used. |
| `Material_Alpha_NoSort` | Skip sorting this object's triangles. Faster, and wrong for overlapping transparent faces. |

## Texturing

| Property | Effect |
| --- | --- |
| `Nif_Texture_ApplyMode` | How the texture meets the vertex and material color: Replace, Decal, Modulate (the usual), Highlight. |
| `Nif_Texture_FilterMode` | Nearest for crisp pixel art, Trilinear for everything that should not shimmer at a distance. |
| `Nif_Texture_ClampMode` | Wrap to tile the texture, Clamp to stop it at the edge, per axis. |
| `Nif_Texture_DarkMap`, `Nif_Texture_DetailMap`, `Nif_Texture_GlossMap`, `Nif_Texture_GlowMap`, `Nif_Texture_BumpMap` | Extra texture slots, by path. |

The dark map slot is where baked lighting goes. Set `Nif_Texture_DarkMap` and your texture wins
over the lightmap for that entity; the compiler says so when it happens.

## Moving textures

Set `Nif_UV_Mode` to **Scroll**, then `Nif_UV_U` and `Nif_UV_V` to how many texture widths per
second the texture moves along each axis: water, lava, a conveyor of doom. The compiler works out a
loop length that ends on whole tiles, so a wrapped texture never visibly jumps back.

Set it to **Oscillate** for a texture that sways back and forth instead: seaweed, a heat shimmer, a
banner in a draft. `Nif_UV_U` and `Nif_UV_V` are then how far it swings each way, in texture widths,
and `Nif_UV_Period` is how many seconds one full swing takes. Oscillate needs a period; without one
the compiler warns and leaves the texture still.

## Structure: groups and links

Some nodes wrap geometry rather than decorate it. Place these as point entities inside a
TrenchBroom group, and they wrap every brush in that group:

| Entity | Wraps the geometry in |
| --- | --- |
| `nif_node_billboard` | A billboard: the geometry turns to face the camera, pivoting at the marker. `Nif_Billboard_Mode` says how: Always Face Camera (the default), Rotate About Up for things that stay upright, Rigid Face Camera, or Always Face Center. Those four are all a Morrowind NIF can hold, and OpenMW draws the last one as Always Face Camera. |
| `nif_node_sort_adjust` | A sort node: `Nif_Sort_Mode` Inherit, Off or Subsort, for transparent geometry that sorts badly. |
| `nif_node_collision_root` | Nothing visible: it moves the mesh's collision node (`RootCollisionNode`) to the marker, without moving the collision itself. One per mesh; any more are ignored with a warning. |

Markers in nested groups nest in the same order: the outer group's node contains the inner group's.
Where a marker sits matters, because it is the node's pivot.

To say which node goes inside which, name one with `Nif_LinkName` and point the other at it with
`Nif_Target`. A brush entity can set `Nif_Target` too, to hang its geometry under a named node
instead of the innermost one. TrenchBroom draws these links as lines. Outside any group a target is
looked up across the whole map; inside one, only within that group's chain.

The compiler refuses, with a message naming the problem:

- a `Nif_Target` that matches no `Nif_LinkName` in its scope, or matches more than one;
- two markers with the same `Nif_LinkName` in one scope;
- targets that loop back on themselves.

A map with no groups and exactly one `nif_node_collision_root` is a special case: that marker
applies to all ungrouped geometry, since there is nothing else it could mean.

## Rotation

`mangle` is TrenchBroom's rotation for an entity: Y-up ZYX Euler angles, in degrees. Morrobroom
converts it to Morrowind's Z-up reference rotation. TrenchBroom keeps it up to date as
you rotate things, so there is rarely a reason to type it.
