+++
title = "Start here"
description = "Install Morrobroom, set up TrenchBroom, and compile a first room into OpenMW."
weight = 10

[extra]
kind = "tutorial"
+++

Morrobroom does not replace OpenMW or TrenchBroom. Each program has one job:

{{ schematic(data_path="data/schematics/workflow.json") }}

You need:

- [OpenMW](https://openmw.org/), with Morrowind's data available to it;
- [TrenchBroom](https://trenchbroom.github.io/);
- Morrobroom, from the [downloads on the front page](@/home/index.md): one zip for your
  platform.

## 1. Unpack Morrobroom

Unzip the archive wherever you keep your tools. There is no installer. Inside:

| Path | What it is |
| --- | --- |
| `morrobroom` (`morrobroom.exe` on Windows) | The compiler. |
| `resources/` | TrenchBroom's Morrowind game configuration: game definition, entity definitions, editor textures and models, and the compile profile. |
| `README.md`, `LICENSE`, `THIRD_PARTY.md` | The usual. |
| `content/` | The source of this manual. |

From a terminal, `morrobroom --help` should list three commands: `compile`, `nif2map` and `fgd`.
If it does, the program works. Everything else in this guide happens inside TrenchBroom.

## 2. Install the TrenchBroom game files

Copy the **contents** of `resources/` into TrenchBroom's custom `Morrowind` game directory:

| Platform | Directory |
| --- | --- |
| Windows | `%APPDATA%\TrenchBroom\games\Morrowind` |
| macOS | `~/Library/Application Support/TrenchBroom/games/Morrowind` |
| Linux | `~/.TrenchBroom/games/Morrowind` |

The directory should end up holding `GameConfig.cfg`, `CompilationProfiles.cfg`, the `.fgd` files,
`Textures/` and `Meshes/`, not a `resources/` folder of its own.

Restart TrenchBroom and pick **Morrowind** from the game list. If it is not there, the files are
not in the directory TrenchBroom is reading.

## 3. Point TrenchBroom at Morrowind

Open **Preferences → Games → Morrowind** and set the game directory to the folder that
**contains** `Data Files/`:

```text
Morrowind/
└── Data Files/
```

Do not select `Data Files/` itself: TrenchBroom looks for textures in `Data Files/textures`, and
it adds the `Data Files` part on its own.

Then set the three compilation tools:

- **Morrobroom**: the program you unpacked;
- **OpenMW**: the OpenMW executable;
- **OpenCS**: optional, for finishing generated plugins in the construction set.

## 4. Build a room

1. Create a new map for **Morrowind**.
2. Build a small room from brushes: a floor, four walls and a ceiling, with no gaps between them.
3. Apply a Morrowind texture to the faces you will see.
4. Place a `Light_Point1024` entity inside the room.
5. **Save the map.** Morrobroom compiles the file on disk, not whatever TrenchBroom is showing.
6. Open **Run → Compile Map**, choose the **Map-to-Engine** profile, and run it.

The profile deletes the map's previous plugin, compiles the map into `build/<map name>/` beside
the `.map` file, and starts OpenMW with that directory as data, straight into the new room.

If OpenMW starts in your room, the pipeline works. Walk around, then go back to TrenchBroom and
change something. If OpenMW shows the room as it was, save the map and compile again: the saved
file is the source of truth.

If it does not work, [Troubleshooting](@/docs/troubleshooting.md) starts with the problems people
meet here.

## Next

[Mapping](@/docs/mapping.md) explains how brushes, faces and entities turn into Morrowind content,
and [Entities](@/docs/entities.md) lists everything you can place. For moving around TrenchBroom
and editing brushes, watch
[DumptruckDS's TrenchBroom playlist](https://youtube.com/playlist?list=PLgDKRPte5Y0AZ_K_PZbWbgBAEt5xf74aE&si=0HrERzygljsiMz2h);
the [official TrenchBroom manual](https://trenchbroom.github.io/manual/latest/) is the reference
once you want more than a room.
