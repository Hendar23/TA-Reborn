# Supreme Commander units

TA Reborn makes the factions of **Supreme Commander: Forged Alliance** (the UEF, the Cybran
Nation, the Aeon Illuminate and the Seraphim) into a mod of TA units, to play in skirmish beside or
against ARM and CORE. As with the maps (docs/SUPCOM_MAPS.md), they are made on the player's
computer from their own copy of the game into their mods folder: nothing of Supreme Commander's is
shipped with TA Reborn or kept in this repository.

This page records what the game's units are made of (`crates/ta_supcom`: [`unit`], [`scm`],
[`sca`]) and how they are made into TA's ([`factions`]): their models, paint, build pictures and
gameplay, and their walks and deaths. Their sounds come later.

- **Players:** the launcher's Mods page, **SUPREME COMMANDER…** (or the Maps page's, its
  FACTIONS tab). The dialog finds the game (as for its maps: GOG's or Steam's copy, or a folder
  chosen), counts each faction's units by kind, with its commander's build picture, and makes the
  factions chosen into the mod **Supreme Commander factions** (`supcom-factions`), turned on, a
  side each in skirmish. All four take about 400 MB and, on a fast computer, ten seconds; a stop
  installs nothing, and the mod is made whole each time, of the factions chosen (those left out
  are taken out of it). The mod keeps the version of the converter that made it
  (`factions::make::VERSION`, its manifest's `version`): a mod made by an older one is marked
  UPDATE. It needs the TA Reborn that made it, or newer (`requires_reborn`).
- **Developers:** every unit of the player's copy read, with what its blueprints name that is not
  there, the conventions below held over all of them, and every unit made a model twice, the same:
  `cargo test --release -p ta_supcom --test installed_units -- --ignored --nocapture`
  (`TA_SUPCOM_DIR=<folder>` another copy). `ta-tools supcom-units <mod folder> [--only UEL0201,..]
  [--faction uef] [--game <folder>] [--preview <dir>]` makes the units' models, surfaces, paint
  and build pictures, with first unit files, into a folder to mount as a mod (`--demo gallery
  TA_GALLERY_UNITS=UEL0201` shows them). The launcher's dialog for captures: `--page mods --show
  supcom-factions` (`supcom-factions:uef,cybran`, or `:all`, imports those once the units are
  listed; `TA_SUPCOM_SHOT_IMPORTING=1` takes the picture while they are made).

## What the game holds

Read from the game's own archives only (`gamedata/*.scd`, `Files::own`): Forged Alliance Forever's
archives rebalance the units, and the import plays them as Forged Alliance does.

- `units.scd`: 679 unit folders, 568 blueprints (`<id>_unit.bp`), 1,148 meshes (`.scm`), 474
  animations (`.sca`) and 3,276 textures (`.dds`).
- **357 units a player can make** ([`UnitBlueprint::playable`]: of the four factions, in the game
  rather than its campaign, built by something (an air transport by its factories'
  `TRANSPORTBUILTBY…` categories) or a commander): UEF 92, Cybran 91, Aeon 92, Seraphim 82, each
  about 26 tech 1, 31 tech 2, 26 tech 3, three or four experimentals, the ACU and a support
  commander. Every one is made by some builder, by its lists' categories.
- Their first levels of detail hold 785,000 vertices in all (the Fatboy, UEL0401, the most: 15,100);
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
| vertices | 68 bytes each: place, normal, tangent, binormal (3 floats each, in the model at rest; the tangent and binormal run against the texture's u and v), two texture coordinates (2 floats each), four bone indices (u8: the first binds it) |
| indices | u16, three a triangle |
| info | text |

A bone's rest pose in the model is its parents' composed with its own ([`Mesh::rest_poses`]): that
matches the inverse the file keeps, bone for bone (held over every unit). Parents always come
before their children, not always depth first.

The axes are right-handed, y up; a unit faces +z, its right at -x (the commanders' `Right_` arms,
the Striker's muzzle forward of its exhausts): TA Reborn's own. A triangle's corners wind
anticlockwise about the normal seen from outside (99% of them; the rest degenerate or folded). No
triangle has corners bound to two bones.

## Models

Each unit's first level of detail becomes `objects3d/<id>.3do` with its surface
(`objects3d/<id>.surface`, docs/FORMATS.md), its paint three DDS textures in `textures/supcom/`:

- **Size:** a mesh unit is `Display.UniformScale` ogrids, an ogrid 32 TA pixels (twice the maps'
  16: a T1 tank about a TA tank's size, the player's choice). The meshes times their scale match the
  blueprints' sizes: the Striker 0.57 × 0.58 × 0.84 ogrids, 18 × 19 × 27 pixels.
- **Pieces:** a bone a piece, depth first (as a 3DO lists them), each laid along the model's axes at
  its bone's place at rest (its offset its place less its parent's), its vertices where they are at
  rest less that place: the model at rest is the mesh exactly, and a piece a script turns turns
  about the model's axes there, as TA's do, whatever turn its bone rests in. z is negated (a 3DO
  holds it the other way round) and each triangle's corners taken the other way round with it.
- **Paint:** the colour, specular and team colour, and normal maps made BC3 of 512 × 512 with every
  level: Forged Alliance's blocks kept wherever a texture has a level of that size (most; 1024² and
  2048² ones from their smaller levels), the rest decoded and made again (256² and smaller grown,
  one-level 1024² ones halved). The units are shaded as `Unit` (UEF), `Insect` (Cybran), `Aeon`
  and `Seraphim` (each LOD's `ShaderName`): the Seraphim keep the player's colour in their colour
  map's alpha (their specular map's empty), which the converter moves into the specular map's, as
  the others have it. Their dark paint mirrors the sky over most of it (the specular map's red):
  TA Reborn draws strong mirroring as polished metal. The normal maps are Direct3D's (x along u, y
  along v, down the picture), as the panel seams of a factory's roof show under a low sun.
- **Pictures:** a unit's build picture (`unitpics/<id>.pcx`, 64 × 64, 24-bit) is its icon (`textures/ui/common/icons/units/<id>_icon.dds`, every unit has one) laid over the frame of its kind (`land_up.dds`, `amph_`, `air_`, `sea_`, by `General.Icon`), as Forged Alliance's build buttons show it. `--preview <dir>` draws a sheet of them a faction.
- **Far off:** the second level of detail (323 units; 687,000 vertices in all against 104,000)
  keeps a few bones of the first's skeleton, its root renamed `lod1_<id>`: it is laid onto the
  first's pieces (a bone the piece of its name, or of the nearest of its parents with one), painted
  with the first's paint, as `objects3d/<id>_lod1.3do`, which the first's surface names `far`.

The 357 units' models come to 56 MB and their paint to 361 MB, made in about seven seconds.

## Gameplay

Each unit becomes a TA unit (`units/<id>.fbi`, `scripts/<id>.cob`; `ta_supcom::factions::units`,
`script`, `faction`, made whole by `make`), its numbers Forged Alliance's at TA's scale:

- **Health, costs, sight:** `Defense.Health`, `Economy.BuildCostMass` and `BuildCostEnergy`
  as they are; intel radii × 32 pixels.
- **Build time:** FA builds in `BuildTime / BuildRate` seconds, TA in `BuildTime /
  WorkerTime`: both taken ten times (`units::WORK`), so a unit takes as long as in FA and
  its builders' work stands beside TA's (a commander's 100 against TA's 300).
- **Movement:** speeds × 32 / 30 (pixels a tick), turn rates in degrees a second to TA's
  angle units a tick, TA's movement classes by how it moves and its footprint, and the fields
  TA reads beside the class (hovercraft hover, ships float, submarines keep to deep water,
  amphibians go to the sea bed). Mobile units take square footprints, as TA's do. Structures
  stand where their build layers let them: on land, afloat (`Floater`), or on the sea bed
  (extractors and hydrocarbon plants on deposits under water as well as on land).
- **Transports:** an air transport carries as many units as its mesh has attach bones for small
  ones (`…Attachpoint…`, `_sml` or `_Small` or plain, on either side; or its larger slots where
  it has none: the UEF's T2 gunship carries one), as large as its largest slots take (TA's
  `TransportSize`). The T1 transports carry 6 to 8, the T2 10 to 16, the UEF's T3 28.
- **Economy:** power generators make energy as in FA; extractors switch on when built, cover a
  metal spot (three cells a side, as TA's) and extract as TA's (0.001), three times that at T2
  and nine times at T3 (FA's 2, 6 and 18 mass a second); fabricators make metal from energy
  while on; radar, sonar, jammers, stealth and shields switch on when built and pay their
  upkeep while on, as TA's do.
- **Weapons** (`weapons/supcom_<faction>.tdf`): up to eight slots, the most TA Reborn gives a
  unit (TA's three and five more: `Weapon4` to `Weapon8`). A commander's main gun goes first and
  its Overcharge second, as TA's D-gun (`commandfire`); otherwise the most damaging go first, an
  anti-air one kept where there is one. 162 of the 357 units carry none, 130 one, 27 two, 15
  three and 23 more than three; four fill all eight (the Fatboy, the CZAR, the Summit and the
  Hauthuum). Weapons an enhancement brings, dummies
  and anti-torpedo defences are left out; a death weapon is what the unit explodes as. Ranges ×
  32, reloads 1 / rate of fire, muzzle velocities × 32, splash diameters twice the damage
  radius × 32, salvos as bursts, damage as it is (a nuke's inner damage over its outer
  radius). Each looks and sounds as TA's weapons of its kind (lasers, shells, artillery,
  missiles, torpedoes, bombs, anti-air, the D-gun, nukes, anti-missiles), TA's own art. A
  missile flies long enough to reach as far as its range (a nuke's, the whole map) half again,
  with its climb to speed.
- **Scripts:** made by an assembler (`ta_formats::cob::Assembler`). A turret turns its yaw bone
  to the target's heading less the heading it rests at, and pitches its pitch bone about the
  axis square to the way it rests (an x turn and a z turn: exact for guns resting ahead,
  behind or to a side, close for the rest; 71 of the 259 turrets rest turned), less the pitch it
  rests at; muzzles take turns, racks kick back along their barrels. The first three weapons answer
  TA's functions (`AimPrimary`... `FireTertiary`), the fourth to the eighth the numbered ones
  (`AimWeapon4`... `FireWeapon8`). Each weapon's turrets turn
  back to rest twice the longest reload after its last aim, by a restore of its own that any
  weapon turning the same pieces stops when it aims. Engineers and commanders lift their build
  stance and spray from their build effect bones, and reclaim; factories open their yards and
  build on the bone their blueprint attaches units to. Fresh units hide what their
  enhancements show.
- **Builds:** each builder's build lists matched against the units' categories. A commander
  builds its own tier (`BUILTBYCOMMANDER`: its engineering enhancements, which bring the others,
  are not yet in TA Reborn); a factory builds units (the buildings its lists name are its own
  upgrades, in place, as are a building's: later). The menus (`download/supcom_<faction>.tdf`)
  put buildings before units, the lower tech first, economy, factories, defences, then the
  rest; the same lists are the computer players' (`[CANBUILD]`).
- **Sides:** `gamedata/sidedata_supcom.tdf` adds UEF, CYBRAN, AEON and SERAPHIM to the game's
  sides (a side add-on: docs/MODDING.md), each with its commander.
- **Wrecks** (`features/corpses/supcom_<faction>.tdf`): the unit's own model where it fell,
  worth its `Wreckage` share of its cost.
- **Animations** (`objects3d/<id>.anims`, ta_formats' `anims`, made by `factions::anims`): a unit's
  `AnimationWalk` as its `walk` clip and its `AnimationDeath` ones as `death`, `death2`... A frame's
  bones are composed down the skeleton (a bone the animation leaves alone keeps its rest under its
  parent), each piece drawn where its bone has gone, turned by its bone's turn less its turn at rest,
  and kept as a script's move and turn under its parent piece. A walk's root keeps to where it
  stands (the unit's own movement carries it; its rise and fall kept), a death's goes with it.
  102 of the 357 units have clips: 29 walks (the commanders, the support commanders, the bots, the
  walking experimentals) and 93 with deaths; 6 MB in all.

The computer players play them: UEF against Cybran on Coast To Coast, twelve minutes (`cargo test
--release -p ta_ai --test skirmish` with `TA_TEST_MOD` and `TA_TEST_SIDES=UEF,CYBRAN`), each
builds an economy, land, air and naval factories, engineers, tanks, artillery, anti-air,
bombers, interceptors and frigates and fights.

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

Seen across the 357 (the installed test lists them); the conversion steps round them:

- Animations that move bones the mesh has not (a death moving an attach point; the frigate
  UES0202's deaths move 17 bones of an older mesh).
- Weapons naming bones their mesh has not (UAA0203's rack, the Czar's `Right_Turret01_Barrel03`,
  UES0202's missile hatches, XRL0403's missile turret, two Seraphim ships' racks).
- Animations named that are not there (URL0103's and XSL0304's deaths, UAS0103's third, named
  `uas00103`).
