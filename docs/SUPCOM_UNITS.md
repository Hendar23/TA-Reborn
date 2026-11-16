# Supreme Commander units

TA Reborn is to make the factions of **Supreme Commander: Forged Alliance** (the UEF, the Cybran
Nation, the Aeon Illuminate and the Seraphim) into a mod of TA units, to play in skirmish beside or
against ARM and CORE. As with the maps (docs/SUPCOM_MAPS.md), they are made on the player's
computer from their own copy of the game into their mods folder: nothing of Supreme Commander's is
shipped with TA Reborn or kept in this repository.

This page records what the game's units are made of, as read so far (`crates/ta_supcom`:
[`unit`], [`scm`], [`sca`]); the conversion and the import come next, phase by phase.

- **Developers:** every unit of the player's copy read, with what its blueprints name that is not
  there: `cargo test --release -p ta_supcom --test installed_units -- --ignored --nocapture`
  (`TA_SUPCOM_DIR=<folder>` another copy).

## What the game holds

Read from the game's own archives only (`gamedata/*.scd`, `Files::own`): Forged Alliance Forever's
archives rebalance the units, and the import plays them as Forged Alliance does.

- `units.scd`: 679 unit folders, 568 blueprints (`<id>_unit.bp`), 1,148 meshes (`.scm`), 474
  animations (`.sca`) and 3,276 textures (`.dds`).
- **348 units a player can make** ([`UnitBlueprint::playable`]: of the four factions, in the game
  rather than its campaign, built by something or a commander): UEF 89, Cybran 89, Aeon 90,
  Seraphim 80, each about 25 tech 1, 30 tech 2, 26 tech 3, three or four experimentals, the ACU
  and a support commander. Every one is made by some builder, by its lists' categories.
- Their first levels of detail hold 754,000 vertices in all (the Fatboy, UEL0401, the most: 15,100);
  194 walks and deaths.
- `projectiles.scd` (289 projectile blueprints, 48 meshes), `textures.scd` (539 build icons,
  1,249 strategic icons), `effects.scd` (2,827 emitter blueprints), `loc_US.scd` (the string
  table; the blueprints carry their English after each key, `'<LOC uel0201_name>MA12 Striker'`),
  `sounds/` (160 XACT banks).

## Blueprints

Lua tables (`UnitBlueprint { ... }`, read as data by [`lua`]). What the conversion uses, kept
typed in [`UnitBlueprint`] (and the whole table beside it):

| Section | Read | Note |
| --- | --- | --- |
| `Categories` | all | faction, tier (`TECH1`..`TECH3`, `EXPERIMENTAL`, `COMMAND`, `SUBCOMMANDER`), kind, and the `BUILTBY*` categories builders' lists name |
| `General` | FactionName, UnitName, BuildBones, UpgradesTo/From | |
| `Defense` | MaxHealth, RegenRate | |
| `Economy` | BuildCostMass/Energy, BuildTime, BuildRate, BuildableCategory, ProductionPerSecondMass/Energy, MaintenanceConsumptionPerSecondEnergy, StorageMass/Energy | a builder makes the units with every category of one of its `BuildableCategory` entries ('BUILTBYTIER1ENGINEER UEF') |
| `Physics` | MotionType, MaxSpeed (and reverse), MaxAcceleration, MaxBrake, TurnRate, BuildOnLayerCaps, Elevation, SkirtSize | speeds in ogrids a second, turns in degrees a second |
| `SizeX/Y/Z`, `Footprint` | sizes, MaxSlope | ogrids |
| `Intel` | Vision, WaterVision, Radar, Sonar, Omni radii; stealth, cloak, stealth fields, jamming | ogrids |
| `Air` | MaxAirspeed | |
| `Weapon` | per weapon: label, WeaponCategory, damage and its radius, Min/MaxRadius, RateOfFire, MuzzleVelocity, BallisticArc, salvo, ProjectileId, FireTargetLayerCapsTable, the turret's bones (TurretBoneYaw/Pitch/Muzzle) and RackBones (rack, muzzles), its arcs and speeds, beams, energy, nukes' rings, ManualFire | |
| `Display` | UniformScale, Mesh.LODs (or the game's own names), AnimationWalk and its rate, AnimationDeath, BuildAttachBone, MeshBlueprint | |
| `Wreckage` | MassMult, EnergyMult, HealthMult | |

A blueprint names its meshes and textures, or the game's own names hold: level `k`'s mesh
`<id>_lod<k>.scm`, the first level's textures `<id>_albedo.dds`, `<id>_specteam.dds` and
`<id>_normalsts.dds`, the others' `<id>_lod<k>_albedo.dds`... A few borrow another's mesh
(`Display.MeshBlueprint`: the walls a prop's, URB3103 URL0101's) or have none (`<none>`: the
concrete pads).

Sizes: a mesh's units times `Display.UniformScale` are ogrids (the Striker, UEL0201: 0.57 by 0.58 by
0.84; its blueprint's `Size` 0.7 by 0.5 by 0.9).

## Meshes (`.scm`, version 5)

Little-endian. Every vertex is bound to one bone, as TA's pieces hold their polygons: a unit's
skeleton is its piece tree.

| Bytes | What |
| --- | --- |
| 0 | `MODL`, version (5) |
| 8 | where the bones are; how many have vertices; where the vertices are; where the extra vertices are (none); how many vertices; where the indices are; how many indices; where the info is and how long; how many bones in all (u32 each) |
| 64 | the bones' names, NUL-ended (after a `NAME` mark at 60, between `0xC5` padding) |
| bones | 108 bytes each: the inverse of the bone's rest pose in the model (4 x 4 floats, rows: the translation in the last), its place and turn relative to its parent (3 floats, then a quaternion w, x, y, z), where its name is (u32), its parent (i32, -1 the root), two words unused |
| vertices | 68 bytes each: place, tangent, normal, binormal (3 floats each, in the model at rest), two texture coordinates (2 floats each), four bone indices (u8: the first binds it) |
| indices | u16, three a triangle |
| info | text |

A bone's rest pose in the model is its parents' composed with its own ([`Mesh::rest_poses`]): that
matches the inverse the file keeps, bone for bone. Parents always come before their children.

## Animations (`.sca`, version 5)

| Bytes | What |
| --- | --- |
| 0 | `ANIM`, version (5), frames, length in seconds (float), bones; where the names are, where the parents are, where the first frame is, how long a frame is (u32 each) |
| names | NUL-ended (after a `NAME` mark) |
| parents | an i32 per bone, -1 the root (after a `LINK` mark) |
| first frame at | the root's place and turn at the start (28 bytes) |
| then the frames | each: its time (float, seconds), flags (u32), then a place and a turn per bone relative to its parent (28 bytes each) |

The bones are named as the mesh's are, not necessarily in its order: an animation is matched to a
mesh by name. A walk loops (its last frame its first: the Hunter's, URL0106, 31 frames in 3
seconds).

## What the game's data gets wrong

Seen across the 348 (the installed test lists them); the conversion steps round them:

- Animations that move bones the mesh has not (a death moving an attach point; the frigate
  UES0202's deaths move 17 bones of an older mesh).
- Weapons naming bones their mesh has not (UAA0203's rack, the Czar's `Right_Turret01_Barrel03`,
  UES0202's missile hatches, XRL0403's missile turret, two Seraphim ships' racks).
- Animations named that are not there (URL0103's and XSL0304's deaths, UAS0103's third, named
  `uas00103`).
