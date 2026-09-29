# Notes for whoever works on the compiler next

What is still open after the 2026-09-29 round of fixes. Everything else these notes used to list is
fixed, with a test, in the commits of that day, including how brush entities read `mangle`, maps
whose worldspawn has no brushes, and the units nif2map writes.

## Declared in the FGDs, not compiled

Both stay in TrenchBroom's entity browser as previews. Their descriptions end in "(preview, not
compiled)", and the compiler prints `<class> is an editor preview and is not compiled yet`.

- **`nif_fx_fire`** (`Nif.fgd`). Compiling it means generating a particle system (NiBSParticleNode,
  NiRotatingParticles, a particle controller and its modifiers) from the `Nif_Fire_*` properties,
  and checking the result in OpenMW and Morrowind; unit tests cannot say whether it looks like fire.
- **The VFX catalog** (`VFX.fgd`, `vfx_kurp_*`). Each entity previews one geometry block of
  Kurpulio's `kurpmesh-uv-y-5sec.nif` (`resources/kurpulio_vfx_inventory.csv` maps entity to
  block). Compiling one means finding that NIF in the user's VFS, which Morrobroom cannot ship and
  does not know the path of, and copying the block out under the entity's NIF properties.

## Loose ends

- **nif2map does not import oscillating UVs.** It recognizes linear Scroll keys only, so a mesh
  compiled with `Nif_UV_Mode` Oscillate comes back with a diagnostic and no `Nif_UV_*` properties.
  Recognizing Morrobroom's own five quadratic keys (0, A, 0, -A, 0 at quarter periods) would do.
- **Catalog light boxes ignore `fgd --scale`.** Non-carryable lights get a box of half their
  radius (`write_fgd_prop.rs`, `impl WriteFGDProp for Light`), which is the right size only at the
  default scale of 2.0; the writer is not given the scale.
- **Catalog display names are FGD-token encoded.** `ESM3_Name` defaults read like
  `Caius_x20_Cosades`. The compiler never reads them for catalog placements, so this is cosmetic.
- **Placed records do not add masters.** A catalog placement references a record from another
  plugin without listing that plugin in the header. OpenMW resolves it by ID; the manual says so.
- **A map with no brushes at all does not compile.** With `--no-lightmaps`, `compile_map` asserts
  "No brushes found in map!"; with lightmaps on, `MapData` panics first, in `constrain_lightmap`,
  on the zero-sized atlas of an empty mesh. Since the cell no longer needs worldspawn brushes, a
  map of nothing but placed records is otherwise compilable; both would have to allow an empty
  render mesh.
