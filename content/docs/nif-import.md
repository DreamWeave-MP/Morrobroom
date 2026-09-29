+++
title = "NIF import"
description = "Reverse-compile visual NIF geometry into editable Valve 220 brush geometry."
weight = 40

[extra]
kind = "guide"
+++

`nif2map` reverse-compiles reconstructible visual NIF geometry into editable
Valve 220 brush geometry. It is not a byte-identical inverse of NIF
serialization: animation, skinning, controllers, and other non-geometric scene
semantics do not become `.map` data.

## What it reads and writes

The input is the NIF's **visual geometry**. Collision nodes are always inspected
and reported, but are excluded from authored map output by default; use
`--include-collision` to emit them in a separate Collision group.

For each input, `nif2map` writes:

- a `.map` file that can be opened in TrenchBroom;
- a `.nif2map.json` report containing reconstruction metadata and recognizer
  results, semantic node scopes, unsupported-state diagnostics, base-texture UV
  set selection, and texture-dimension provenance.

The result is editable geometry, not a promise that the original NIF's scene
behavior or object semantics will survive the trip.

NIF UV coordinates are preserved as authored. `nif2map` does not invert V for
Valve 220 output; image-orientation normalization belongs to the texture loader.
`--texture-root` is repeatable and accepts either an OpenMW data directory or an
existing `.bsa`, `.ba2`, or `.zip` archive. These sources form one OpenMW-style
VFS: archives are registered first, then loose directories, and later sources
win within each class. Missing textures are errors; dimensions are never guessed.

Texture references follow OpenMW's `correctTexturePath` policy. A reference is
normalized, rooted under `textures/` unless it already names `textures/` or
`bookart/`, and `.dds` is tried before the referenced extension. The flat
top-level fallback is then tried. Thus `tx_akula_face00.tga` resolves
`textures/tx_akula_face00.dds` before `textures/tx_akula_face00.tga`, including
when the DDS comes from an archive.

## Structural and surface reconstruction

`nif2map` first tries to recognize **structural** shapes: geometry that can be
represented directly as a small, meaningful set of convex brush solids.

Shapes that are not reconstructed structurally use **planar-prism surface
reconstruction** by default. This can recover useful visual geometry from
beds, trees, heads, weapon racks, Taris, and other shapes that are not a good
fit for the structural recognizers. A single source shape may become many
planar-prism brushes.

Use `--fallback skip` when you only want geometry accepted by the structural
recognizers. The default `--fallback planar-prisms` is the better choice when
the goal is visual inspection or an editable blockout.

Complex NIFs can produce enormous `.map` files. That is expected for detailed
surface reconstruction, and it is why the converter has a brush limit. Start
with `--dry-run --verbose` when importing a large scene and inspect the report
before opening the result in TrenchBroom.

## A normal import

```bash
morrobroom nif2map meshes/ \
  --recursive \
  --texture-root "/path/to/Morrowind/Data Files" \
  --texture-root "/path/to/Morrowind/Data Files/Morrowind.bsa" \
  --texture-root "/path/to/Morrowind/Data Files/Tribunal.bsa" \
  --texture-root "/path/to/Morrowind/Data Files/Bloodmoon.bsa" \
  --output-dir nif2map-out
```

With `--recursive`, each output keeps the input's path relative to the scanned
directory. For example, `Meshes/a/foo.nif` becomes
`nif2map-out/a/foo.map` and `nif2map-out/a/foo.nif2map.json`.

Then open the generated `.map` in TrenchBroom, inspect the geometry, and save a
copy before making edits. `nif2map` is a reverse-compilation workflow in its
own right; you can use it to recover visual geometry for a new level, inspect
an existing asset, or produce a blockout for further brush authoring.

## Scale

A NIF is in Morrowind units; a map is in map units, half as large, because
`compile` multiplies by its `--scale` of `2.0`. `nif2map` divides by the same
scale, so its maps sit at the size of everything else you build in
TrenchBroom, and compiling one at the default scale gives the NIF back at its
own size. If you compile at another `--scale`, pass the same one to `nif2map`.

## Options that matter

| Option | Default | Purpose |
| --- | --- | --- |
| `--texture-root PATH` | | **Required.** Add an ordered data directory or `.bsa`, `.ba2` or `.zip` archive to the OpenMW-style texture VFS; repeatable. |
| `-r`, `--recursive` | | Scan input directories recursively and preserve their relative subdirectories in the output. |
| `-o`, `--output-dir PATH` | `nif2map-out` | Write `.map` and `.nif2map.json` files there. |
| `--fallback MODE` | `planar-prisms` | `planar-prisms` surface reconstruction, or `skip` for structural-only output. |
| `--max-brushes N` | `20000` | Stop if one input would generate more than `N` brushes. |
| `-s`, `--scale NUMBER` | `2.0` | Divide the NIF's Morrowind units by this to write map units. Keep it the same as `compile --scale`. |
| `--shell-thickness N` | `8` | Backing thickness for open swept architectural shells, in map units. |
| `--fallback-thickness N` | `1` | Thickness used by planar-prism surface reconstruction, in map units. |
| `--skip-material NAME` | `skip` | The material for the closing and partition faces reconstruction adds, which were never visible in the NIF. |
| `--include-collision` | | Emit `RootCollisionNode` descendants in a separate Collision group. |
| `--overwrite` | | Permit existing outputs to be replaced. |
| `--dry-run` | | Analyze without writing map files. |
| `-v`, `--verbose` | | Print recognizer details and warnings. |

`--no-validate` disables generated-brush validation and is intended for
diagnosing converter problems, not normal imports. Validation is one of the
things preventing a difficult NIF from poisoning the resulting map.
