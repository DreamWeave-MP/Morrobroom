# Notes for whoever works on the compiler next

Found on 2026-09-28 while rewriting the manual against the source. The manual describes what the
code does today; these are the places where that differs from what the FGDs, comments or older docs
promised.

## Bugs

All four fixed on 2026-09-28: the bake reads `ESM3_light_color`, `ESM3_Ambient_color` and
`ESM3_Radius`; `parse_string` lets `'` appear inside `"..."`; the FGD now describes `ESM3_Model` as
the generated mesh's output path. `clip` needed nothing: visible geometry comes from the render mesh,
which drops it, so a `clip` face only collides.

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
- **`Nif_Billboard_Mode` other than 0.** `insert_render_marker` (`mesh.rs:237`) prints "cannot store
  billboard mode" and writes the default NiBillboardNode; the FGD offers seven modes.
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
