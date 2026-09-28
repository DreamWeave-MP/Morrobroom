# Notes for whoever works on the compiler next

Found on 2026-09-28 while rewriting the manual against the source. The manual describes what the
code does today; these are the places where that differs from what the FGDs, comments or older docs
promised. Nothing below has been changed.

## Bugs

- **The lightmap bake ignores the colors and radii set in TrenchBroom.** The FGD writes
  `ESM3_light_color` on point lights and `ESM3_Ambient_color` on worldspawn, and the light and cell
  *records* read those (`game_object.rs`, via `get_prop`, which adds the `ESM3_` prefix). The bake
  reads the bare keys instead: `light_color` in `collect_point_lights` (`map_data.rs:253`) and
  `Ambient_color` in `collect_ambient_color` (`map_data.rs:274`). So a light placed in TrenchBroom
  has the right color in game but bakes white, and the baked ambient is always 15 15 15. The bake
  also takes a light's radius from its class name only; the record honors an `ESM3_Radius`
  override. The lightmap test fixtures pass because they were written by hand with the bare keys.
- **`ESM3_Model` does the opposite of what the FGD says.** `Morrowind.fgd` describes it as the
  model the object uses, with brush geometry only when it is empty. `process_brush_entity`
  (`main.rs:239`) uses it as the path the *generated* mesh is written to, relative to `Meshes/`.

## Declared in the FGDs, not compiled

Each of these appears in TrenchBroom's entity browser, and placing one compiles to nothing:
`process_point_entities` prints `Unidentified point entity class: <class>` and moves on.

- **Records placed from the generated catalog** (`MorrowindObjects.fgd`, from `morrobroom fgd`).
  The catalog is useful for browsing and previews, but only `Light_Point*`, `world_CreatureList`
  and `world_ItemList` point entities become references. `info_player_start` is a scale reference
  and also reaches that branch.
- **`nif_fx_fire`** and the **VFX catalog** (`VFX.fgd`, Kurpulio's meshbank previews).
- **`Nif_UV_Mode` Oscillate.** Only Scroll is built (`mesh.rs:629`); Oscillate prints a warning and
  `Nif_UV_Period` is unused.
- **Armor and clothing.** `item_Armor` is commented out in `Morrowind.fgd`.
- **Smooth Shading**, the face attribute `GameConfig.cfg` itself marks "not yet implemented".

## Loose ends

- `broom_args.rs:145` justifies the `FGD` variant with "FGD is the established command spelling and
  appears in the CLI contract", but clap exposes the subcommand as `fgd` and the tests call it
  that. The manual used to say `morrobroom FGD`, which fails with "unrecognized subcommand"; it now
  says `fgd`. Either rename the command or reword the comment.
- `morrobroom --help` describes `nif2map` but not `compile` or `fgd`: their variants have no doc
  comments.
- `fgd --types` rejected the uppercase record tags the manual showed. Commit 2bce2e5 made it
  case-insensitive; that is the only code change made during the docs work.
