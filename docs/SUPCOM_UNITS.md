# Supreme Commander units

TA Reborn makes the factions of **Supreme Commander: Forged Alliance** (the UEF, the Cybran
Nation, the Aeon Illuminate and the Seraphim) into a mod of TA units, to play in skirmish beside or
against ARM and CORE. As with the maps (docs/SUPCOM_MAPS.md), they are made on the player's
computer from their own copy of the game into their mods folder: nothing of Supreme Commander's is
shipped with TA Reborn or kept in this repository.

As with the maps, the import is an **experimental feature**: the launcher shows it with **ENABLE
EXPERIMENTAL FEATURES** on (Settings, LAUNCHER; off by default). A factions mod made before stays
installed and plays as any mod; `--experimental`, or `--show supcom-factions`, shows the import for
one run.

This page records what the game's units are made of (`crates/ta_supcom`: [`unit`], [`projectile`],
[`behaviour`], [`motions`], [`scm`], [`sca`]) and how they are made into TA's ([`factions`]): their
models, paint, build pictures and gameplay, their walks and deaths, and what their buildings do
as they work. Their sounds come later.

- **Players:** the launcher's Mods page, **SUPREME COMMANDER…** (or the Maps page's, its
  FACTIONS tab). The dialog finds the game (as for its maps: GOG's or Steam's copy, or a folder
  chosen, kept for the next start), counts each faction's units by kind, with its commander's
  build picture, and makes the factions chosen into the mod **Supreme Commander factions**
  (`supcom-factions`), turned on, a side each in skirmish. All four take about 440 MB (3,638
  files) and, on a fast computer, about half a minute, most of it writing and installing their
  files; the room on the disk is checked first. A stop installs nothing (not once the mod is being
  installed: STOP is greyed then), and the mod is made whole each time, of the factions chosen
  (those left out are taken out of it); a unit that cannot be made is named in the dialog. It goes
  on with the dialog closed (the launcher's footer shows it), and an update of the launcher waits
  for it; the mod cannot be removed or replaced meanwhile. Like the maps' pack, it is made again
  where the launcher finds it, and in place of another mod of its id only once the player agrees.
  The mod keeps the version of the converter that made it (`factions::make::VERSION`, its
  manifest's `version`): a mod made by an older one is marked UPDATE, in the dialog and on the Mods
  page (its card, the UPDATES filter, UPDATE…). It needs the TA Reborn that made it, or newer
  (`requires_reborn`).
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
  commander. Every one but the tech 2 and 3 factories is made by some builder, by its lists'
  categories; those Forged Alliance makes only by upgrading the factory below
  (`General.UpgradesTo`), as TA Reborn does (Upgrades, below): every one is then reached from
  its faction's commander, by builds and upgrades (UEF 13 upgrades, the others 12 each).
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
| `Weapon` | per weapon: label, WeaponCategory, damage and its radius, Min/MaxRadius, RateOfFire, MuzzleVelocity, BallisticArc, salvo (MuzzleSalvoSize, MuzzleSalvoDelay, MuzzleChargeDelay), RackFireTogether, RackSalvoReloadTime and RackSalvoChargeTime, DoTPulses and DoTTime, ProjectileId, FireTargetLayerCapsTable, TargetRestrictOnlyAllow, the turret's bones (TurretBoneYaw/Pitch/Muzzle) and RackBones (rack, muzzles), its arcs and speeds, beams (BeamLifetime, BeamCollisionDelay), energy, CountedProjectile and MaxProjectileStorage, nukes' rings, ManualFire | |
| `Display` | UniformScale, Mesh.LODs (or the game's own names), AnimationWalk and its rate, AnimationDeath, BuildAttachBone, MeshBlueprint | |
| `Wreckage` | MassMult, EnergyMult, HealthMult | |

A weapon's projectile (`projectiles.scd`, `<name>_proj.bp`, its `ProjectileId`; [`projectile`])
says what kind of shot it is (its `Categories`: a `STRATEGIC` or `TACTICAL` `MISSILE` is what
missile defences take), what a counted one costs to build (`Economy`: a nuke 12,000 mass and
1,350,000 energy over 324,000 of its silo's build power) and how it flies (`Physics`: its speeds,
`TrackTarget`, `TurnRate`). What a projectile splits into as it lands is its script's (beside its
blueprint: `OnImpact` making `numProjectiles` children; [`behaviour::fragments`]): the
Salvation's shell six, each of those six more.

What a unit's script switches off as it is made (`units/<id>/<id>_script.lua`;
[`behaviour::birth`]): its weapons' `OnCreate` and its own `OnCreate` and `OnStopBeingBuilt`, in
turn (`SetWeaponEnabled(false)`, `SetWeaponEnabledByLabel`, `EnableUnitIntel`,
`DisableUnitIntel`). Switched by the layer it stands on (an `if` there, or its `OnLayerChange`:
the Wagner's guns on land and torpedo under the water) counts as on. Off at birth in this copy:
the Seraphim commanders' missiles (an enhancement's), the toggled second guns (the sniper's, the
Cybran ships' and anti-air's ground guns), the Megalith's hack-peg launcher, the Cybran
commanders' stealth, cloak and sonar, the Seraphim scout's stealth and cloak, the UEF support
commander's jammer.

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
  along v, down the picture): across the meshes' seams, where the two sides' texture coordinates
  run different ways, the normals they give agree best read so (the meshes' own tangents and
  binormals run against u and v).
- **Light:** Forged Alliance lights its paint in its texels' own (gamma) space, by its map's
  lighting multiplier and sun (about 1.3 each on its maps): a lit face of the UEF's (`Unit`, the
  sky its specular map's red mirrors on top) some 1.6 times as bright as its texel, of the Cybran's
  (`Insect`, the sun taken twice) 2.3 times, of the Aeon's (`Aeon`, six tenths of the sun, the
  mirrored sky on top) and the Seraphim's (`Seraphim`, their look the sky they mirror) about as
  bright as it. TA Reborn lights a colour map as the colour of what is painted (a lit face about 1.3
  times it, in light's own measure), so the converter brightens each by its shader
  (`textures::paint_gain`: the UEF's 1.45 times, the Cybran's twice, the others as they are) by
  multiplying each block's two colour ends, Forged Alliance's blocks otherwise as they were made:
  drawn as they were, the UEF's and the Cybran's units were half as bright as in their game. The
  specular map's green is how strongly a broad, cool sheen shines over the paint where the sun
  glances off it towards the eye (Forged Alliance's highlights on its units' metal, a power of 2 or
  3 broad: what makes that paint read as metal), the player's colour is laid on as the game lays
  it (the paint blended into the player's own colour as far as the alpha says), and a model's
  rim light, and its highlight when selected, go by its faces' own normals: lit by every turn of
  its normal map, a selected Commander was a pale haze, the ground seeming to show through it.
- **Texture coordinates** run 0 to 1 (TA Reborn keeps them in 16 bits, and a texture's edge
  clamps): a triangle the mesh paints from a repeat of its texture is moved by whole textures to
  lie within it (the same texels), its corners copied where other triangles share them: 229 of
  them (4 in the far models). A triangle straddling the texture's edge cannot be, and is left as
  it is: 162 (59 in the far models), at most a few hundredths of a model's surface.
- **Pictures:** a unit's build picture (`unitpics/<id>.pcx`, 64 × 64, 24-bit) is its icon (`textures/ui/common/icons/units/<id>_icon.dds`, every unit has one) laid over the frame of its kind (`land_up.dds`, `amph_`, `air_`, `sea_`, by `General.Icon`), both over the dark grey of a build menu's panel as their alphas say (the frame's rounded corners and rim are see-through, white in its texels), as Forged Alliance's build buttons show it: all 357 within a step of the game's own icon over its frame. `--preview <dir>` draws a sheet of them a faction. Both interfaces show these pictures for these units (the modern one draws TA's units' portraits from their models instead): a portrait drawn before its model's paint was read stayed a dark silhouette in the build menu, and portraits drawn elsewhere (the unit viewer, a cockpit's screen) now wait for their paint.
- **Far off:** the second level of detail (332 units; 717,000 vertices in all against 107,000)
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
- **Mending:** a unit that mends itself (`Defense.RegenRate`, 15 of them: the commanders, the
  support commanders, the experimentals) has TA's `HealTime`, 100 / RegenRate (TA Reborn mends
  100 hit points every `HealTime` seconds).
- **Movement:** speeds × 32 / 30 (pixels a tick), turn rates in degrees a second to TA's
  angle units a tick (FA's own, however slow: the Atlantis's 12°), TA's movement classes by how
  it moves and its footprint, and the fields TA reads beside the class (hovercraft hover, ships
  float, submarines keep to deep water). An amphibian takes no class (TA's amphibious ones go
  no deeper than 100): its own `MaxWaterDepth=255` and `MaxSlope`, to the deepest sea bed. Land
  units take square footprints, as TA's do; a ship or a submarine a square of its footprint's
  area (a frigate's 2 by 6 ogrids, 4 by 12 cells, 7 by 7; a battleship 18; TA's ships stand on
  squares less than their length), its wreck blocking nothing, as TA's sea wrecks don't.
  Structures stand where their build layers let them: on land, afloat (`Floater`), or on the
  sea bed (extractors and hydrocarbon plants on deposits under water as well as on land). A
  structure on land stands on a footprint as wide as what of its model stands on the ground
  (`model::ground_extent`: what lies from a quarter of an ogrid under its foot to half an ogrid
  over it) where that is wider than its blueprint's (`units::fitted_footprint`; walls keep theirs,
  to stand in line): Forged Alliance's footprints are what its units may not cross, often smaller
  than the building (a point defence's 0.6 ogrids under a turret of a whole one), the ground round
  them levelled for them and kept clear (their skirts), while TA Reborn levels the ground drawn
  under a footprint alone, so a model reaching past its footprint ran into the hillside beside
  it, and its neighbours could be built into it. They stand on slopes as steep as TA's buildings
  do (`MaxSlope=10`), the ground drawn under them levelled and the slope taken up round them.
- **Aircraft** fly as FA flies them, at TA's scale (`Air`): their top speed `MaxAirspeed`; to it
  in a second and a half times their `KMoveDamping` over their `KMove` (an interceptor 1.5 s,
  0.36 px a tick², TA's Freedom Fighter's 0.35; a bomber 3, TA's Thunder 3.8; a transport 3.2),
  braking ten times as fast; a winged one turning at its `TurnSpeed` (radians a second: an
  interceptor 86° a second, TA's Freedom Fighter 84°; a bomber 40°), a hovering one at its
  `Physics.TurnRate`, else twice its `KTurn`. Gunships (`GROUNDATTACK`) and the hovering craft
  with guns (the CZAR) hover over what they attack (`HoverAttack`); fighters and bombers make
  passes.
- **Transports:** an air transport says it carries as many units as its mesh has attach bones
  for small ones (`…Attachpoint…`, `_sml` or `_Small` or plain, on either side; or its larger
  slots where it has none: the UEF's T2 gunship one), as large as its largest slots take (TA's
  `TransportSize`): the T1 transports 6 to 8, the T2 10 to 16, the UEF's T3 28. TA Reborn's air
  transports carry one unit, whatever their files say, as TA's did (TA's Atlas says 5 as well):
  so do these.
- **Economy:** power generators make energy as in FA; extractors switch on when built, cover a
  metal spot (three cells a side, as TA's), store nothing (as TA's) and extract as TA's (0.001),
  three times that at T2 and nine times at T3 (FA's 2, 6 and 18 mass a second); fabricators
  make metal from energy while on; radar and sonar stations (and sonar platforms) switch on when
  built and pay their upkeep while on, as TA's do. A hydrocarbon plant stands on a vent (the
  converted maps put one at each hydrocarbon point), every cell of its yard map a `G`, as TA's
  geothermal plants. Upkeep for what TA Reborn does not play yet (shields, the Fatboy's among
  them, stealth fields, a unit's own stealth and cloak, jamming, remote viewing) is left out
  with it: a stealthy unit is hidden from radar for nothing, as TA's are.
- **Weapons** (`weapons/supcom_<faction>.tdf`): up to eight slots, the most TA Reborn gives a
  unit (TA's three and five more: `Weapon4` to `Weapon8`). A commander's main gun goes first and
  its Overcharge second, as TA's D-gun (`commandfire`); otherwise the most damaging go first, an
  anti-air one kept where there is one. Weapons an enhancement brings, those the unit's script
  switches off as it is made (a toggled second gun keeps the one it starts with), dummies,
  anti-torpedo defences and weapons of a point a shot (the Galactic Colossus's tractor claws)
  are left out; a death weapon is what the unit explodes as. Ranges × 32, muzzle velocities ×
  32 (a missile's its projectile's top speed), splash diameters twice the damage radius × 32
  (FA's blasts deal their damage evenly to their edge: flak's edge 0.9), a nuke's inner damage
  over its outer radius. FA's minimum ranges are not TA's: a weapon fires as close as it likes.
  Each looks and sounds as TA's weapons of its kind (lasers, shells, artillery, missiles,
  torpedoes, bombs, anti-air, the D-gun, nukes, anti-missiles), TA's own art.
- **Firing** as FA's rules fire it (its `lua/sim/defaultweapons.lua`: `units::cycle`): each time
  it fires one rack fires, in turn, or all of them (`RackFireTogether`); a rack fires its
  muzzles one after another `MuzzleSalvoSize` times, or all of them at once (`MuzzleSalvoDelay`
  0); it fires `RateOfFire` times a second, no faster than its muzzles' charges and delays
  allow, waits `RackSalvoReloadTime` and charges `RackSalvoChargeTime` once every rack has
  fired, and fires no faster than its energy comes (`EnergyRequired` at
  `EnergyDrainPerSecond`). A shot deals its `Damage` each of its damage over time's pulses
  (`DoTPulses`), as many times as its projectile splits into pieces, all the way down (the
  Salvation's 36), or as its beam hits (every `BeamCollisionDelay` and a tick for its
  `BeamLifetime`; a beam kept on, `BeamLifetime` 0, every such while it has a target: the CZAR's
  and the Galactic Colossus's). In TA's keys: a burst of the shots it fires each time, a reload
  giving FA's damage a second (in whole ticks, the damage taken a little up or down for them):
  every one of the 308 slots within half a percent of FA's. A shell's pieces land where it does,
  together (TA has no shells that split).
- **Missiles** its launcher builds and stores (`CountedProjectile`: the nukes, the subs' and the
  Hauthuum's strategic missiles, the tactical missile launchers, the anti-nukes) are TA's
  stockpile: each built at its projectile's price (`metalpershot`, `energypershot`) over its build
  time at the launcher's build rate (`reloadtime`: a nuke 300 s, an anti-nuke 240, a tactical
  missile 30), up to TA Reborn's ten. The strategic and tactical launchers' missiles fly as TA's
  strategic ones: upright out of the tube (a nuke for five seconds, topping out some 1,300 px over
  the ground; a tactical missile for three, some 670 px, about as high as TA's own
  `CORTRON_WEAPON`'s 770), then level across the map (`cruise`), half a turn a second, down onto
  their target from above; a nuke, without `tracks`, at the place it is sent (TA's nukes strike a
  place), a tactical missile homing on what it is sent at. A missile leaving its tube upright (an
  anti-nuke's too) leaves at 120 px a second at least, clear of the ground before it arms; where
  its muzzle rests under the ground (FA's silos open and lift their missile: the Aeon's lies 100
  px down), from the silo's foot. A submarine fires its missiles from under the water
  (`firesubmersed`). A missile flies long enough to reach as far as its range (a nuke's, the whole
  map) half again, with its climb to speed; a nuke at TA's nukes' 350 pixels a second at least
  (FA's leave their silo at a crawl and speed up on the way, which a TA missile cannot).
- **Missile defences** take what FA's take (TA's `interceptor` and `targetable` as classes,
  docs/FORMATS.md): strategic missiles (`STRATEGIC MISSILE` projectiles) `targetable=1`, the
  anti-nukes (`TargetRestrictOnlyAllow = 'STRATEGIC MISSILE'`) `interceptor=1`; tactical missiles
  (`TACTICAL MISSILE`: the launchers', the missile trucks', the cruisers') `targetable=2`, the
  tactical missile defences and ships' point defences (`'TACTICAL MISSILE'`, `'TACTICAL,MISSILE'`)
  `interceptor=2`, at their own rate of fire and unstocked. A defence fires once a missile is
  within its range (`MaxRadius`, as FA's), its shot leaving at full speed: its projectile's, and
  at least the Seraphim's 2,880 px a second (`COUNTERMEASURE_SPEED`): the Cybran zapper is a beam
  and the Aeon's flares draw missiles onto them, which TA Reborn's interceptors do not do (each
  flies at the missile it takes), and at the flares' 160 px a second one never caught a tactical
  missile coming down at 384.
- **Anti-air** homes on its aircraft (TA Reborn's guns lead nothing: an unguided shot at where a
  plane is now misses it), `toairweapon`: missiles at their projectile's speeds and turn (TA's
  missile towers' turn at least), guns and flak as homing shots, flak bursting over its real
  area. A weapon that shoots no aircraft in FA (its `FireTargetLayerCapsTable` names no `Air`)
  and could hit one in TA Reborn (a straight shot, a laser, a guided missile) takes aircraft
  last (its slot's bad targets, `VTOL`: TA's rule, bad targets last), and a unit none of whose
  weapons shoots aircraft chases none (`NoChaseCategory=VTOL`).
- **A crawling bomb** (the Fire Beetle, its `Kamikaze` weapon) is TA's: `Kamikaze=1`, blowing
  itself up as its blast (`SelfDestructAs`) within its weapon's reach of an enemy
  (`KamikazeDistance`).
- **Scripts:** made by an assembler (`ta_formats::cob::Assembler`). A turret turns its yaw bone
  to the target's heading less the heading it rests at, and pitches its pitch bone about the
  axis square to the way it rests (an x turn and a z turn: exact for guns resting ahead,
  behind or to a side, close for the rest; 71 of the 259 turrets rest turned), less the pitch it
  rests at; muzzles take turns, racks kick back along their barrels. The first three weapons answer
  TA's functions (`AimPrimary`... `FireTertiary`), the fourth to the eighth the numbered ones
  (`AimWeapon4`... `FireWeapon8`). Each weapon's turrets turn
  back to rest twice the longest reload after its last aim, by a restore of its own that any
  weapon turning the same pieces stops when it aims. Engineers and commanders lift their build
  stance and spray from their build effect bones, and reclaim; a weapon turning the pieces the
  build turns (a commander's torso) declines to aim while it builds, as TA's Commander's does,
  and leaves them to the build; factories open their yards and build on the bone their blueprint
  attaches units to. Fresh units hide what their enhancements show, and all under it. A rack
  kicks back as the muzzle on it fires.
- **Builds:** each builder's build lists matched against the units' categories. A commander
  builds its own tier (`BUILTBYCOMMANDER`), not what FA keeps from it until its engineering
  enhancements (`BUILTBYTIER2COMMANDER`, `BUILTBYTIER3COMMANDER`: the Cybran commander's
  Engineering Station), which add it to its menu (below); a factory builds units (the
  buildings its lists name are its own upgrades, in place). A building FA makes only by
  upgrading the one below (no builder's lists reach it: the tech 2 and 3 factories) is on no
  builder's menu: it is made by upgrading, as in FA. An engineering station (the Kennel, the
  Hive) lends its build power to what is built round it (TA's assist turret: a builder with no
  menu); an air staging platform repairs the aircraft landed on it, as TA's repair pads do
  (`IsAirBase`). A mobile factory (the Fatboy, the carriers, the Atlantis, the Tempest, the CZAR)
  builds its units out in the field round it, as an engineer does: TA Reborn has no factory that
  moves. The menus (`download/supcom_<faction>.tdf`) put buildings before units, the lower tech
  first, economy, factories, defences, then the rest; the same lists are the computer players'
  (`[CANBUILD]`).
- **Upgrades** (`faction::upgrade_of`): a unit whose `General.UpgradesTo` names a unit of its
  faction that is made, and that has build power to upgrade with (`Economy.BuildRate`), says
  `UpgradesTo` (docs/MODDING.md): the tech 1 factories into the tech 2 and those into the tech 3
  (land, air and naval), the extractors (tech 1 into 2 into 3), the radars (into the omni
  sensor), the sonars (into the tech 3 sonar platform) and the UEF's and the Seraphim's tech 2
  shield generators: 49 in all (what the others name is no unit a player makes). A building
  that builds nothing else gets its `BuildRate` as its `WorkerTime` for it; a fabricator, which
  names an upgrade and has none, never upgrades (nor does it in FA). An upgrade takes what FA's
  does: the new unit's build time at the old one's build rate (a tech 1 land factory into the tech
  2 in 80 s, that into the tech 3 in 210 s; a tech 1 extractor into the tech 2 in 90 s, that into
  the tech 3 in about 190 s), at the new unit's price. A factory makes nothing while it upgrades,
  an extractor extracts on, a radar watches, as in FA.
- **Enhancements** (`factions::enhance`; the unit file's `[ENHANCEMENTS]`, docs/MODDING.md): the
  commanders' and support commanders' `Enhancements`, each with its slot, prerequisite, price and
  build time (FA's times ten, as a unit's: the UEF's tech 2 suite takes its commander 90 s), and
  its button's picture (`unitpics/<unit>_<id>.pcx`), in the order of their ids. What each does is
  read from its blueprint and from the unit script's `CreateEnhancement` branch for it (as data):
  health and mending (`NewHealth`, `NewRegenRate`; a chain's adding up, as FA's buffs stack, an
  engineering suite's replacing the one before, as its buff does), build power (`NewBuildRate`,
  the difference from the unit's own), the units FA keeps from a commander until then
  (`BuildableCategoryAdds`, a suite past the first keeping the first's), weapons switched on
  (`EnabledByEnhancement`, or the script's `SetWeaponEnabledByLabel`: in slots past the unit's own,
  empty on a fresh unit; switched off by a later one in the chain, as the UEF's Billy takes the
  tactical launcher's place), the main gun changed (`NewMaxRadius`, `NewRateOfFire`, the damage and
  its radius: a weapon of its own in the gun's slot, the Overcharge reaching as far), sight, an
  omni sensor's radius as radar and sonar, sonar, stealth and cloak the script switches on. The
  script shows the bones an enhancement's `ShowBones` names, and all under them, once it is
  installed (`SetEnhancement`). 42 enhancements are played, on the four commanders (the UEF's 6, the
  Cybran's 7, the Aeon's 5, the Seraphim's 7) and their support commanders (3, 6, 3 and 5). Left out, with what TA Reborn does not play
  yet: shields (the personal ones and the bubbles), teleporting, drones and pods, resource
  allocation, regeneration fields, the Sacrificial System, the EMP burst, the radar jammer, and the
  Chrono Dampener (a weapon of a point a shot): one none of whose effects is played is not offered,
  nor one needing such another. FA's "Remove" enhancements are TA Reborn's own removal (at once,
  for nothing).
- **Sides:** `gamedata/sidedata_supcom.tdf` adds UEF, CYBRAN, AEON and SERAPHIM to the game's
  sides (a side add-on: docs/MODDING.md), each with its commander, drawn in Arm's panels (the UEF
  and the Aeon) or Core's (the Cybran and the Seraphim). They have no interface pages of their
  own: the classic interface lays their build menus out as the pages of the side whose panels they
  wear (its Commander's first build page, six pictures to a page, the page arrows under them), and
  takes its orders and main pages from that side too.
- **Wrecks** (`features/corpses/supcom_<faction>.tdf`): the unit's own model where it fell,
  worth its `Wreckage` share of its mass (`MassMult`), in metal alone (`energy=0`), as TA's wrecks
  are: TA Reborn pays a wreck's metal alone whatever its file says (docs/FORMATS.md), and none of
  Forged Alliance's units' wreckage gives energy anyway (`EnergyMult = 0` in all 504). It is drawn
  burnt out (sooted, its player's colour gone to dark grey, its lights dead: TA's wrecks are
  painted so, these are the units' own paint).
- **Animations** (`objects3d/<id>.anims`, ta_formats' `anims`, made by `factions::anims`): a unit's
  `AnimationWalk` as its `walk` clip and its `AnimationDeath` ones as `death`, `death2`... A frame's
  bones are composed down the skeleton (a bone the animation leaves alone keeps its rest under its
  parent), each piece drawn where its bone has gone, turned by its bone's turn less its turn at rest,
  and kept as a script's move and turn under its parent piece. A walk's root keeps to where it
  stands (the unit's own movement carries it; its rise and fall kept), a death's goes with it.
  A walk is a cycle, its last frame its first: five of FA's end out of step with their start
  (UEL0106's knees 36° out, URL0107's leg 38°, UAL0106 14°, UEL0303 9°, URL0301 6°), eased back
  over the cycle's last quarter rather than jumping there each step. 102 of the 357 units have
  clips: 29 walks (the commanders, the support commanders, the bots, the walking experimentals)
  and 93 with deaths; 6 MB in all.
- **Buildings at work** (`factions::work`, from the blueprints and from what each unit's script
  moves, [`motions`]): what Forged Alliance moves on a structure as it works becomes what TA
  Reborn moves on it. Its keyframed animations become clips of its model's (`active`, `open` and
  `upgrade`, played by the game: docs/FORMATS.md), each from what plays it in Forged Alliance:
  - `active`, a loop played while the building works (finished, on, its upkeep paid; a factory
    while it builds), slowing to a stop where it is as it stops: the animation its script plays
    round and round (`PlayAnim(anim, true)`, or again in a loop: every extractor's pump, the
    Seraphim hydrocarbon plant's at the half rate it sets) or its class does (the Cybran land and
    air factories' `AnimationBuild` while they build);
  - `open`, played once as it starts working and held, back as it stops: what its script plays
    once (the Aeon fabricator's opening, the Hive's as it builds, the Aeon nuke silo's arms), its
    `AnimationPermOpen` (played once it is finished: the anti-nukes, the Aeon hydrocarbon plant,
    the Cybran perimeter monitor) and the Seraphim shield generators' `AnimationActivate` (as their
    shields come up). A clip turning a part a weapon aims, or one that carries it, is not made:
    the turrets that unfold before they aim (the Aeon's point defence, artillery, flak and SAMs,
    the Cybran torpedo ambusher) stand as they rest and aim from there, as before;
  - `upgrade`, its `AnimationUpgrade`, posed at the share of its upgrade in place done, as
    Forged Alliance sets the animation's frame from the new building's progress while it hides
    that building: the tech 1 and 2 factories, extractors, radars and sonars and the UEF's shield
    generator. Done, the new building stands in its place and starts its own work.

  The parts its script spins (`CreateRotator`) and slides (`CreateSlider`) become its TA script's
  `spin`s and `move`s (in the simulation: they are the same on every computer), about the model's
  axis the bone's own turns about (spins are signed by the right hand, as the factories'
  `RollOffPoints` turn their units: `UnitSpin` 90 towards +x), at Forged Alliance's speeds and
  accelerations: from the unit's making (`OnCreate`: the Seraphim factories' pods), as it works
  (`OnStopBeingBuilt`, `OnIntelEnabled`, `OnProductionUnpaused`, `OnShieldEnabled` and the states
  they change into; stopped as it is switched off: `Activate` and `Deactivate`, or once its build
  is done where it has no switch), or as it builds (a factory's `StartArmsMoving`: `StartBuilding`
  and `StopBuilding`). Speeds set both ways in a loop make a swing (the Cybran radars' dishes,
  tilting back and forth about their own slanted axes, made of turns about x and z; the Aeon
  fabricator's spinner reversing); places set in a loop a shuttle (the factories' gantries); the
  UEF power generator slides its panels out before its drums spin up. A factory's build point
  slid aside (the Aeon, UEF and Seraphim tech 3 and UEF tech 2 naval factories') stays where it
  was: it is where the simulation builds their units. Of the 186 structures a player can make, 66
  play clips (19 `active`, 13 `open`, 44 `upgrade`) and 46 move parts in their scripts, all of them
  made (`cargo test --release -p ta_supcom --test installed_units buildings_at_work -- --ignored
  --nocapture` lists them). Mobile units' (an engineer's arms, an aircraft's hatches) are not yet.

The computer players play them (`cargo test --release -p ta_ai --test skirmish` with
`TA_TEST_MOD` and `TA_TEST_SIDES=UEF,CYBRAN`): each builds an economy, land, air and naval
factories, engineers, tanks, artillery, anti-air, bombers, interceptors and frigates and fights,
and climbs the tiers by upgrading its factories in place: a factory's tier is the highest tech of
the engineers it makes, and on the economy the first advanced factory waits for, one factory at
a time is upgraded into the next tier (`SkirmishAi::factory_tier`, `run_upgrades_in_place`; past
the first, once a constructor of the top tier is out), more of them once metal piles up; and it
upgrades its safe extractors, a few at a time as its income and power allow (on Sherwood, UEF
against Cybran: the tech 2 factories at 9 minutes, the UEF's tech 3 at 16, the first tech 2
extractors at 5 and 8). Their commanders install the engineering suites and a gun upgrade (one that
reaches farther, hits harder or fires faster), at home and when the economy allows, its energy
within a minute's income (`run_enhancements`; on Sherwood: the tech 2 suite at 10 to 16 minutes,
the gun at 11 to 18, the tech 3 suite at 17 to 26). They fire a tactical launcher's
missiles at what is worth one in reach (never a wall, nor what a tactical defence covers), and
build anti-nukes against strategic silos alone. Experimentals they do not build yet.

Not yet in TA Reborn: Forged Alliance's own sounds and weapon effects (TA's of their kinds play),
shields, stealth fields and teleporting, and the enhancements that need them (and drones,
resource allocation, regeneration fields); and, besides, jamming, remote viewing, a weapon's toggles (the sniper's second
gun), FA's minimum ranges, factories that move and build within themselves, and air transports
carrying more than one unit at a time.

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
  `uas00103`; the UEF hydrocarbon plant's opening, `ueb1102_Aopen`, which its script plays as it
  starts: it opens no hatch; the tech 3 extractors' upgrades, which they never make).
