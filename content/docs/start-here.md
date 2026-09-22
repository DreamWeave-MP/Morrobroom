+++
title = "Start Here"
description = "Install Morrobroom, configure TrenchBroom, and compile a first room."
weight = 10

[extra]
kind = "guide"
+++

## The workflow

Morrobroom does not replace OpenMW or TrenchBroom. Each tool has one job:

```text
TrenchBroom .map  →  Morrobroom  →  NIFs + OpenMW plugin  →  OpenMW
     edit              compile          generated assets       play
```

You need:

- [OpenMW](https://openmw.org/), with Morrowind data available to it;
- [TrenchBroom](https://trenchbroom.github.io/);
- a Morrobroom release binary.

The Morrobroom archive includes the TrenchBroom game definition, textures,
editor models, compilation profile, and a base `Morrowind.fgd` under
`resources/`.

## Install the TrenchBroom game files

Copy the contents of Morrobroom's `resources/` directory into TrenchBroom's
custom `Morrowind` game directory:

| Platform | Directory |
| --- | --- |
| Windows | `%APPDATA%\TrenchBroom\games\Morrowind` |
| macOS | `~/Library/Application Support/TrenchBroom/games/Morrowind` |
| Linux | `~/.TrenchBroom/games/Morrowind` |

Restart TrenchBroom and select **Morrowind**. If it is not in the game list,
the files are not in the directory TrenchBroom is reading.

Open **Preferences → Games → Morrowind** and set the game directory to the
folder that **contains** `Data Files/`:

```text
Morrowind/
└── Data Files/
```

Do not select `Data Files/` itself. Then configure the compilation tools:

- **Morrobroom** — the downloaded Morrobroom executable;
- **OpenMW** — the OpenMW executable;
- **OpenCS** — optional, if you want to finish generated plugins there.

The supplied **Map-to-Engine** profile compiles the current map into a local
`build/<map-name>/` directory and launches OpenMW with that directory as data.

## Make your first room

1. Create a new **Morrowind** map.
2. Build a small enclosed room from brushes.
3. Apply a texture to the visible faces.
4. Place a `Light_Point1024` entity inside the room.
5. **Save the map.** Morrobroom compiles the on-disk `.map` file.
6. Open the compile dialog and run **Map-to-Engine**.

If OpenMW starts in the room, the pipeline is working. If you see an older
version, save the map and compile again; the saved file is the source of truth.

For a first introduction to moving around TrenchBroom and editing brushes, use
[DumptruckDS's TrenchBroom playlist](https://youtube.com/playlist?list=PLgDKRPte5Y0AZ_K_PZbWbgBAEt5xf74aE&si=0HrERzygljsiMz2h).
The [official TrenchBroom manual](https://trenchbroom.github.io/manual/latest/)
is the reference once you want more than a room.
