+++
title = "NIF Import"
description = "Reverse-compile visual NIF geometry into editable Valve 220 brush geometry."
weight = 35

[extra]
kind = "workflow"
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
When exact texel dimensions matter, point `--texture-root` at the same visible
asset directory that TrenchBroom uses. Missing textures are reported and use the
import fails rather than guessing a dimension.

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
  --output-dir nif2map-out
```

Then open the generated `.map` in TrenchBroom, inspect the geometry, and save a
copy before making edits. `nif2map` is a reverse-compilation workflow in its
own right; you can use it to recover visual geometry for a new level, inspect
an existing asset, or produce a blockout for further brush authoring.

## Options that matter

| Option | Purpose |
| --- | --- |
| `--recursive` | Scan input directories recursively. |
| `--output-dir PATH` | Write `.map` and `.nif2map.json` files there. |
| `--texture-root PATH` | **Required.** Add a TrenchBroom-visible texture root for source dimensions; repeatable. |
| `--fallback MODE` | Choose `planar-prisms` surface reconstruction or `skip` for structural-only output. |
| `--max-brushes N` | Stop if one input would generate more than `N` brushes. |
| `--shell-thickness N` | Backing thickness for open swept architectural shells. |
| `--fallback-thickness N` | Thickness used by planar-prism surface reconstruction. |
| `--include-collision` | Emit `RootCollisionNode` descendants in a separate Collision group. |
| `--overwrite` | Permit existing outputs to be replaced. |
| `--dry-run` | Analyze without writing map files. |
| `--verbose` | Print recognizer details and warnings. |

`--no-validate` disables generated-brush validation and is intended for
diagnosing converter problems, not normal imports. Validation is one of the
things preventing a difficult NIF from poisoning the resulting map.
