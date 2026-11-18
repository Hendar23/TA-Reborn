//! A unit's script (`scripts/<id>.cob`), made from its blueprint and its mesh: TA's engine calls
//! the same functions of every unit's script (docs/FORMATS.md, "COB bytecode"), and these answer
//! them as the unit's bones say.
//!
//! - **Weapons** (TA's three slots, `AimPrimary`... `FireTertiary`, and five more, `AimWeapon4`...
//!   `FireWeapon8`, as TA Reborn calls a mod's script's): a turret turns its yaw bone to
//!   the target's heading less the heading it rests at, and pitches its pitch bone about the axis
//!   square to the way it rests (an x turn and a z turn, `cos` and `sin` of that heading made
//!   numbers in the script: exact for guns resting ahead, behind or to a side, close for the
//!   rest), less the pitch it rests at; the model is the mesh at rest, so a rear or side gun stands
//!   as it should wherever no script turns it (wrecks, pictures). The muzzles of its racks take
//!   turns, the rack kicking back along its barrel; twice the longest reload after its last aim, it
//!   turns back to rest. A weapon without a turret is always aimed.
//! - **Builders** lift their build stance on `StartBuilding` (an engineer turning its build bones
//!   to the work), and the spray leaves from their build effect bones in turn. Factories open on
//!   `Activate` and build on the piece their blueprint attaches units to. A weapon turning the
//!   pieces the build turns (a commander's torso) declines to aim while it builds, as TA's
//!   Commander's does, and its restore leaves them to the build.
//! - A unit leaves a wreck (`Killed`); a fresh one hides the pieces its enhancements show, and all
//!   under them (as Forged Alliance hides a bone with its children).
//! - **Moving parts** (a building's, from its Forged Alliance script: [`super::work`]): what spins
//!   or slides from the unit's making does so from `Create`; what does as it works, once it is
//!   built (`StartWorking`, from `Activate` where the unit has a switch, else once its build is
//!   done) and until it stops (`StopWorking`, from `Deactivate`): spins speeding up and slowing
//!   down at their acceleration, swings to and fro, slides out (all of them before the spins
//!   start) and back; a factory's arms as it builds (`StartBuilding`, `StopBuilding`).

use ta_formats::cob::{Assembler, CobScript, Function, X_AXIS, Y_AXIS, Z_AXIS, op};

use super::work::{Motions, Sliding, Spinning};
use crate::convert::trig::atan2;
use crate::motions::When;
use crate::scm::{self, Mesh};
use crate::unit::{UnitBlueprint, WeaponBlueprint};

/// The name the engine calls a weapon slot's function by (`kind`: `Aim`, `AimFrom`, `Query` or
/// `Fire`): TA's for the first three slots (`AimPrimary`), Spring's for the rest (`AimWeapon4`),
/// as TA Reborn reads them (ta_data's `WEAPON_FUNCTIONS`).
fn weapon_function(kind: &str, slot: usize) -> String {
    match slot {
        0..=2 => format!("{kind}{}", ["Primary", "Secondary", "Tertiary"][slot]),
        _ => format!("{kind}Weapon{}", slot + 1),
    }
}

/// The unit values scripts set and get (ta_sim's `host::value`).
const IN_BUILD_STANCE: i32 = 5;
const BUILD_PERCENT_LEFT: i32 = 17;
const YARD_OPEN: i32 = 18;

/// The signals a unit's work and its builds' moving parts answer (past the weapons' own, `2 <<
/// slot`).
const SIG_WORK: i32 = 1 << 12;
const SIG_BUILD: i32 = 1 << 13;
/// How long a unit waiting for its build to be done to start working sleeps between looks (ms).
const BUILT_LOOK: i32 = 250;

/// TA's angle units a turn.
const TURN: f64 = 65536.0;
/// cos and sin are numbers of 1024ths in the script.
const ONE: f64 = 1024.0;

/// What a unit's script is made for: the unit, its mesh (the bones' rest poses), its model's
/// pieces (the mesh's bones in the model's order), the weapons in TA's slots, and how its build
/// power is used.
pub struct Plan<'a> {
    pub unit: &'a UnitBlueprint,
    pub mesh: &'a Mesh,
    pub pieces: Vec<String>,
    pub weapons: Vec<&'a WeaponBlueprint>,
    /// It builds standing in place (a factory: `Activate` opens it).
    pub factory: bool,
    /// It builds (an engineer, a commander, a factory).
    pub builder: bool,
    /// TA pixels a mesh unit (its `UniformScale` times 32).
    pub pixels: f64,
    /// The bones each of its enhancements shows (`ShowBones`), by the index the engine gives it
    /// (`factions::enhance`'s order): `SetEnhancement(index, on)` shows them and all under them,
    /// or hides them.
    pub enhancements: Vec<Vec<String>>,
    /// The parts it spins and slides as it is made, works and builds ([`super::work::motions`]).
    pub motions: super::work::Motions,
    /// It has a switch, and is switched on once built (`Activate`) and off by its player
    /// (`Deactivate`): it works while it is on.
    pub switched: bool,
}

/// Where a bone points at rest: its heading (TA angle units, +16,384 towards +x, as the engine's
/// headings count) and the cos and sin of it in 1024ths, and how far up it points (angle units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestAim {
    pub heading: i32,
    pub cos: i32,
    pub sin: i32,
    pub pitch: i32,
}

/// Where `bone` points at rest: its local +z turned by its rest pose (a bone pointing straight up
/// or down has no heading: 0).
pub fn rest_aim(mesh: &Mesh, bone: usize) -> RestAim {
    let rest = mesh.rest_poses();
    let f = scm::rotate(rest[bone].1, [0.0, 0.0, 1.0]);
    let (x, y, z) = (f[0] as f64, f[1] as f64, f[2] as f64);
    let level = (x * x + z * z).sqrt();
    let units = |radians: f64| -> i32 { (radians * TURN / (2.0 * std::f64::consts::PI)).round() as i32 };
    if level < 1e-3 {
        return RestAim { heading: 0, cos: ONE as i32, sin: 0, pitch: units(atan2(y, level)) };
    }
    RestAim { heading: units(atan2(x, z)), cos: (z / level * ONE).round() as i32, sin: (x / level * ONE).round() as i32, pitch: units(atan2(y, level)) }
}

/// Each weapon slot's restore, turning its turrets back to rest: `RestoreAfterDelay`, then
/// `RestoreAfterDelay2`...
fn restore_function(slot: usize) -> String {
    match slot {
        0 => "RestoreAfterDelay".to_string(),
        _ => format!("RestoreAfterDelay{}", slot + 1),
    }
}

/// Degrees a second as TA's angle units a second.
fn angle_speed(degrees: f64) -> i32 {
    (degrees.max(1.0) * TURN / 360.0).round() as i32
}

/// The script.
pub fn script(plan: &Plan) -> CobScript {
    let mut asm = Assembler::new(plan.pieces.clone());
    let piece = |name: &str| -> Option<u32> { plan.pieces.iter().position(|p| p.eq_ignore_ascii_case(name)).map(|i| i as u32) };
    let bone = |name: &str| -> Option<usize> { plan.mesh.bone(name) };
    let reload = asm.static_var();
    // (Each weapon's turrets to turn back to rest, and their speeds: a restore of its own each.)
    let mut rests: [Vec<(u32, u32, i32)>; super::units::MAX_WEAPONS] = Default::default();
    // The pieces an engineer's or a commander's build stance turns, and whether it is building
    // (a weapon turning them leaves them to the build meanwhile).
    let build_pieces: Vec<u32> = if plan.builder && !plan.factory {
        let (yaw, pitch, _) = plan.unit.build_bones.clone().unwrap_or((None, None, Vec::new()));
        [yaw, pitch].into_iter().flatten().filter_map(|b| piece(&b)).collect()
    } else {
        Vec::new()
    };
    let building = (!build_pieces.is_empty()).then(|| asm.static_var());

    // Create: hidden what the enhancements show, and all under it; a reload of a second until told;
    // its moving parts where it is made, and those moving from then on.
    let mut create = Function::new(0);
    create.push(1000).pop_static(reload);
    for p in hidden_pieces(plan) {
        create.hide(p);
    }
    let motions = &plan.motions;
    for s in &motions.slides {
        for k in 0..3 {
            if s.made[k] != 0 {
                create.push(s.made[k]).move_now(s.piece, k as u32);
            }
        }
    }
    start_motions(&mut asm, &mut create, motions, When::Made, 0, false);
    // (Its work starts as TA's engine switches it on, where it does once the unit is built: not a
    // factory, which it switches on as it opens to build.)
    let activated = plan.switched && !plan.factory;
    if motions.any(When::Working) && !activated {
        let built = asm.declare("WorkWhenBuilt");
        create.start_script(built, 0);
    }
    create.ret(0);
    asm.define("Create", create);
    if motions.any(When::Working) {
        work_functions(&mut asm, motions, activated);
    }
    let mut set_reload = Function::new(1);
    set_reload.push_local(0).pop_static(reload).ret(0);
    asm.define("SetMaxReloadTime", set_reload);

    let mut shares_build = [false; super::units::MAX_WEAPONS];
    for (slot, w) in plan.weapons.iter().enumerate().take(super::units::MAX_WEAPONS) {
        let signal = 2i32 << slot;
        let yaw = w.yaw_bone.as_deref().and_then(|b| piece(b).zip(bone(b)));
        let pitch = w.pitch_bone.as_deref().and_then(|b| piece(b).zip(bone(b)));
        // Its muzzles in turn, each with the rack it is on.
        let mut muzzles: Vec<(u32, Option<u32>)> = Vec::new();
        for rack in &w.racks {
            for m in &rack.muzzles {
                if let Some(p) = piece(m) {
                    muzzles.push((p, piece(&rack.rack)));
                }
            }
        }
        if muzzles.is_empty()
            && let Some(p) = w.muzzle_bone.as_deref().and_then(piece)
        {
            muzzles.push((p, None));
        }
        // A missile leaving its tube upright from a muzzle resting under the ground (Forged
        // Alliance's silo opens and lifts it; the Aeon's lies 100 px down) leaves from the
        // silo's foot instead: it would go off in its tube.
        if super::units::launched_upright(w) {
            let rest = plan.mesh.rest_poses();
            let height = |p: u32| plan.pieces.get(p as usize).and_then(|name| plan.mesh.bone(name)).map_or(0.0, |b| rest[b].0[1] as f64 * plan.pixels);
            muzzles.retain(|&(p, _)| height(p) >= 0.0);
            if muzzles.is_empty() {
                muzzles.push((0, None));
            }
        }
        let aim_from = yaw.map(|(p, _)| p).or(pitch.map(|(p, _)| p)).or(muzzles.first().map(|m| m.0)).unwrap_or(0);
        let next = asm.static_var();

        if w.turreted && (yaw.is_some() || pitch.is_some()) {
            let rest = yaw.map(|(_, b)| rest_aim(plan.mesh, b)).unwrap_or(RestAim { heading: 0, cos: ONE as i32, sin: 0, pitch: 0 });
            let barrel = pitch.map(|(_, b)| rest_aim(plan.mesh, b)).unwrap_or(rest);
            let (yaw_speed, pitch_speed) = (angle_speed(w.yaw_speed), angle_speed(w.pitch_speed));
            let mut aim = Function::new(2);
            // (While it builds, a weapon turning the build's pieces declines to aim.)
            if let Some(b) = building
                && [yaw, pitch].iter().flatten().any(|(p, _)| build_pieces.contains(p))
            {
                let go = aim.label();
                aim.push_static(b).jump_if_false(go);
                aim.ret(0);
                aim.mark(go);
                shares_build[slot] = true;
            }
            aim.push(signal).op(op::SIGNAL).push(signal).op(op::SET_SIGNAL_MASK);
            if let Some((y, _)) = yaw {
                // turn yaw to (heading - its rest heading)
                aim.push(yaw_speed).push_local(0).push(rest.heading).op(op::SUB).turn(y, Y_AXIS);
                rests[slot].push((y, Y_AXIS, yaw_speed));
            }
            if let Some((p, _)) = pitch {
                // The pitch, signed (the engine hands 0..65535), less the barrel's own at rest.
                let q = aim.local();
                let down = aim.label();
                aim.push_local(1).pop_local(q);
                aim.push_local(1).push(32767).op(op::SET_GREATER).jump_if_false(down);
                aim.push_local(1).push(65536).op(op::SUB).pop_local(q);
                aim.mark(down);
                aim.push_local(q).push(barrel.pitch).op(op::SUB).pop_local(q);
                // About x by -q cos, about z by -q sin (as the script gives a z turn: negated).
                aim.push(pitch_speed).push_local(q).push(rest.cos).op(op::MUL).push(-(ONE as i32)).op(op::DIV).turn(p, X_AXIS);
                rests[slot].push((p, X_AXIS, pitch_speed));
                if rest.sin != 0 {
                    aim.push(pitch_speed).push_local(q).push(rest.sin).op(op::MUL).push(-(ONE as i32)).op(op::DIV).turn(p, Z_AXIS);
                    rests[slot].push((p, Z_AXIS, pitch_speed));
                }
            }
            if let Some((y, _)) = yaw {
                aim.wait_turn(y, Y_AXIS);
            }
            if let Some((p, _)) = pitch {
                aim.wait_turn(p, X_AXIS);
                if rest.sin != 0 {
                    aim.wait_turn(p, Z_AXIS);
                }
            }
            let restore = asm.declare(&restore_function(slot));
            aim.start_script(restore, 0).ret(1);
            asm.define(&weapon_function("Aim", slot), aim);
        }

        let mut from = Function::new(1);
        from.push(aim_from as i32).pop_local(0).ret(0);
        asm.define(&weapon_function("AimFrom", slot), from);

        // Query: the muzzle whose turn it is.
        let mut query = Function::new(1);
        let done = query.label();
        query.push(muzzles.first().map_or(aim_from, |m| m.0) as i32).pop_local(0);
        for (k, (m, _)) in muzzles.iter().enumerate().skip(1) {
            let skip = query.label();
            query.push_static(next).push(k as i32).op(op::SET_EQUAL).jump_if_false(skip);
            query.push(*m as i32).pop_local(0).jump(done);
            query.mark(skip);
        }
        query.mark(done).ret(0);
        asm.define(&weapon_function("Query", slot), query);

        // Fire: the rack of the muzzle that fired kicks back along its barrel and returns; the next
        // muzzle's turn.
        let mut fire = Function::new(0);
        let kick = (w.rack_recoil.abs() * plan.pixels).min(8.0);
        if kick > 0.1 {
            let rest = pitch.or(yaw).map(|(_, b)| rest_aim(plan.mesh, b)).unwrap_or(RestAim { heading: 0, cos: ONE as i32, sin: 0, pitch: 0 });
            let mut racks: Vec<u32> = muzzles.iter().filter_map(|m| m.1).collect();
            racks.sort_unstable();
            racks.dedup();
            // (The muzzle counter names the muzzle that fired: its rack is the one to kick. With a
            // rack alone, that one.)
            let kicks: Vec<(Option<usize>, u32)> = if racks.len() > 1 {
                muzzles.iter().enumerate().filter_map(|(m, (_, r))| r.map(|r| (Some(m), r))).collect()
            } else {
                racks.iter().map(|&r| (None, r)).collect()
            };
            for (muzzle, rack) in &kicks {
                let (muzzle, rack) = (*muzzle, rack);
                let skip = fire.label();
                if let Some(m) = muzzle {
                    fire.push_static(next).push(m as i32).op(op::SET_EQUAL).jump_if_false(skip);
                }
                // Back along the barrel, in 16.16 pixels: x negated as scripts give it.
                let back = |share: i32| -> i32 { (kick * 65536.0 * share as f64 / ONE).round() as i32 };
                fire.push(back(rest.sin)).move_now(*rack, X_AXIS);
                fire.push(-back(rest.cos)).move_now(*rack, Z_AXIS);
                let speed = (kick * 65536.0 * 4.0).round() as i32;
                fire.push(speed).push(0).move_to(*rack, X_AXIS);
                fire.push(speed).push(0).move_to(*rack, Z_AXIS);
                fire.mark(skip);
            }
        }
        if muzzles.len() > 1 {
            // next = (next + 1) % muzzles
            let wrap = fire.label();
            fire.push_static(next).push(1).op(op::ADD).pop_static(next);
            fire.push_static(next).push(muzzles.len() as i32).op(op::SET_LESS).jump_if_false(wrap);
            fire.ret(0);
            fire.mark(wrap).push(0).pop_static(next);
        }
        fire.ret(0);
        asm.define(&weapon_function("Fire", slot), fire);
    }

    // Each weapon's turrets back to rest twice the longest reload after its last aim. Its restore
    // answers the signal of every weapon turning any of its pieces, so an aim of any of them stops
    // it: a weapon gone quiet never turns back a turret another is aiming.
    for (slot, own) in rests.iter().enumerate().filter(|(_, own)| !own.is_empty()) {
        let mask = rests.iter().enumerate().filter(|(_, other)| other.iter().any(|o| own.iter().any(|s| s.0 == o.0))).fold(0i32, |mask, (j, _)| mask | (2 << j));
        let mut back = Function::new(0);
        back.push(mask).op(op::SET_SIGNAL_MASK);
        back.push_static(reload).push(2).op(op::MUL).op(op::SLEEP);
        // (Building meanwhile, the build has the pieces: it turns them back itself once done.)
        if let Some(b) = building
            && shares_build[slot]
        {
            let go = back.label();
            back.push_static(b).jump_if_false(go);
            back.ret(0);
            back.mark(go);
        }
        for &(p, axis, speed) in own {
            back.push(speed).push(0).turn(p, axis);
        }
        back.ret(0);
        asm.define(&restore_function(slot), back);
    }

    if plan.builder {
        build_functions(&mut asm, plan, building);
    }

    // SetEnhancement(index, on): an enhancement installed shows its parts and all under them, one
    // taken out hides them (TA Reborn's engine calls it).
    let parts: Vec<Vec<u32>> = plan.enhancements.iter().map(|bones| pieces_under(plan, bones)).collect();
    if parts.iter().any(|p| !p.is_empty()) {
        let mut f = Function::new(2);
        for (k, pieces) in parts.iter().enumerate().filter(|(_, p)| !p.is_empty()) {
            let next = f.label();
            f.push_local(0).push(k as i32).op(op::SET_EQUAL).jump_if_false(next);
            let hide = f.label();
            f.push_local(1).jump_if_false(hide);
            for &p in pieces {
                f.show(p);
            }
            f.ret(0);
            f.mark(hide);
            for &p in pieces {
                f.hide(p);
            }
            f.ret(0);
            f.mark(next);
        }
        f.ret(0);
        asm.define("SetEnhancement", f);
    }

    // A wreck where it dies.
    let mut killed = Function::new(2);
    killed.push(1).pop_local(1).ret(0);
    asm.define("Killed", killed);
    asm.finish()
}

/// The build functions: the stance, the spray's pieces, a factory's opening and pad. `building`:
/// the static that says it is building, for the weapons sharing its pieces.
fn build_functions(asm: &mut Assembler, plan: &Plan, building: Option<u32>) {
    let piece = |name: &str| -> Option<u32> { plan.pieces.iter().position(|p| p.eq_ignore_ascii_case(name)).map(|i| i as u32) };
    let (yaw_bone, pitch_bone, effects) = plan.unit.build_bones.clone().unwrap_or((None, None, Vec::new()));
    let mut sprays: Vec<u32> = effects.iter().filter_map(|b| piece(b)).collect();
    if sprays.is_empty() {
        sprays.push(0);
    }
    let spray = asm.static_var();
    let set = |f: &mut Function, id: i32, value: i32| {
        f.push(id).push(value).op(op::SET_VALUE);
    };
    if plan.factory {
        let mut on = Function::new(0);
        set(&mut on, YARD_OPEN, 1);
        set(&mut on, IN_BUILD_STANCE, 1);
        on.ret(0);
        asm.define("Activate", on);
        let mut off = Function::new(0);
        set(&mut off, IN_BUILD_STANCE, 0);
        set(&mut off, YARD_OPEN, 0);
        off.ret(0);
        asm.define("Deactivate", off);
        let pad = plan.unit.display.build_attach_bone.as_deref().and_then(piece).unwrap_or(0);
        let mut info = Function::new(1);
        info.push(pad as i32).pop_local(0).ret(0);
        asm.define("QueryBuildInfo", info);
        let mut start = Function::new(2);
        start_building(asm, &mut start, &plan.motions);
        start.ret(0);
        asm.define("StartBuilding", start);
        let mut stop = Function::new(0);
        if plan.motions.any(When::Building) {
            stop_motions(&mut stop, &plan.motions, When::Building, SIG_BUILD);
        }
        stop.ret(0);
        asm.define("StopBuilding", stop);
    } else {
        let yaw = yaw_bone.as_deref().and_then(|b| piece(b).zip(plan.mesh.bone(b)));
        let pitch = pitch_bone.as_deref().and_then(|b| piece(b).zip(plan.mesh.bone(b)));
        let mut start = Function::new(2);
        if let Some(b) = building {
            start.push(1).pop_static(b);
        }
        if let Some((y, b)) = yaw {
            let rest = rest_aim(plan.mesh, b);
            start.push(angle_speed(180.0)).push_local(0).push(rest.heading).op(op::SUB).turn(y, Y_AXIS).wait_turn(y, Y_AXIS);
        }
        if let Some((p, _)) = pitch {
            start.push(angle_speed(180.0)).push(0).turn(p, X_AXIS);
        }
        set(&mut start, IN_BUILD_STANCE, 1);
        start_building(asm, &mut start, &plan.motions);
        start.ret(0);
        asm.define("StartBuilding", start);
        let mut stop = Function::new(0);
        if plan.motions.any(When::Building) {
            stop_motions(&mut stop, &plan.motions, When::Building, SIG_BUILD);
        }
        set(&mut stop, IN_BUILD_STANCE, 0);
        if let Some(b) = building {
            stop.push(0).pop_static(b);
        }
        if let Some((y, _)) = yaw {
            stop.push(angle_speed(90.0)).push(0).turn(y, Y_AXIS);
        }
        stop.ret(0);
        asm.define("StopBuilding", stop);
    }
    // The spray from each effect bone in turn.
    let mut nano = Function::new(1);
    let done = nano.label();
    nano.push(sprays[0] as i32).pop_local(0);
    for (k, s) in sprays.iter().enumerate().skip(1) {
        let skip = nano.label();
        nano.push_static(spray).push(k as i32).op(op::SET_EQUAL).jump_if_false(skip);
        nano.push(*s as i32).pop_local(0);
        nano.mark(skip);
    }
    nano.mark(done);
    if sprays.len() > 1 {
        let wrap = nano.label();
        let out = nano.label();
        nano.push_static(spray).push(1).op(op::ADD).pop_static(spray);
        nano.push_static(spray).push(sprays.len() as i32).op(op::SET_LESS).jump_if_false(wrap);
        nano.jump(out);
        nano.mark(wrap).push(0).pop_static(spray);
        nano.mark(out);
    }
    nano.ret(0);
    asm.define("QueryNanoPiece", nano);
}

/// What moves as a builder builds, set going in its `StartBuilding` `f` (what moved for the unit
/// before stopped first: `StartBuilding` comes for each unit a factory makes).
fn start_building(asm: &mut Assembler, f: &mut Function, motions: &Motions) {
    if motions.any(When::Building) {
        f.push(SIG_BUILD).op(op::SIGNAL);
        start_motions(asm, f, motions, When::Building, SIG_BUILD, false);
    }
}

/// `StartWorking` and `StopWorking` (see the module's notes), and what calls them: `Activate` and
/// `Deactivate` where the unit is `switched`, else `WorkWhenBuilt`, which `Create` starts.
fn work_functions(asm: &mut Assembler, motions: &Motions, switched: bool) {
    let mut start = Function::new(0);
    // (Started again before it was done: the one before ends.)
    start.push(SIG_WORK).op(op::SIGNAL).push(SIG_WORK).op(op::SET_SIGNAL_MASK);
    start_motions(asm, &mut start, motions, When::Working, SIG_WORK, true);
    start.ret(0);
    let start_id = asm.define("StartWorking", start);
    let mut stop = Function::new(0);
    stop_motions(&mut stop, motions, When::Working, SIG_WORK);
    stop.ret(0);
    let stop_id = asm.define("StopWorking", stop);
    if switched {
        let mut on = Function::new(0);
        on.start_script(start_id, 0).ret(0);
        asm.define("Activate", on);
        let mut off = Function::new(0);
        off.call_script(stop_id, 0).ret(0);
        asm.define("Deactivate", off);
    } else {
        // (Waiting for its build to be done.)
        let mut built = Function::new(0);
        let (look, done) = (built.label(), built.label());
        built.mark(look).push(BUILD_PERCENT_LEFT).op(op::GET_UNIT_VALUE).jump_if_false(done);
        built.push(BUILT_LOOK).op(op::SLEEP).jump(look);
        built.mark(done).call_script(start_id, 0).ret(0);
        asm.define("WorkWhenBuilt", built);
    }
}

/// The axes a slide moves its piece along: those it is made or goes off its rest along.
fn slid_axes(s: &Sliding) -> impl Iterator<Item = usize> + '_ {
    (0..3).filter(move |&k| s.made[k] != 0 || s.goals.iter().any(|g| g[k] != 0))
}

/// The slide's piece sent to `to` at `speed` along each axis (at once where it has none).
fn slide_to(f: &mut Function, s: &Sliding, to: [i32; 3], speed: [i32; 3]) {
    for k in slid_axes(s) {
        if speed[k] > 0 {
            f.push(speed[k]).push(to[k]).move_to(s.piece, k as u32);
        } else {
            f.push(to[k]).move_now(s.piece, k as u32);
        }
    }
}

/// Waits for the slide's piece to arrive (along the axes it moves at a speed).
fn wait_slide(f: &mut Function, s: &Sliding, speed: [i32; 3]) {
    for k in slid_axes(s) {
        if speed[k] > 0 {
            f.wait_move(s.piece, k as u32);
        }
    }
}

/// What moves `when` set going in `f`: its slides to their places (and, `wait`, arrived before its
/// spins speed up); its spins; its swings and shuttles in threads of their own, answering `sig`
/// (a signal of 0: never stopped).
fn start_motions(asm: &mut Assembler, f: &mut Function, motions: &Motions, when: When, sig: i32, wait: bool) {
    let name = match when {
        When::Made => "Made",
        When::Working => "Work",
        When::Building => "Build",
    };
    let slides: Vec<&Sliding> = motions.slides.iter().filter(|s| s.when == when && !s.goals.is_empty()).collect();
    for (k, s) in slides.iter().enumerate() {
        if s.shuttle {
            let thread = shuttle(asm, s, &format!("{name}Shuttle{k}"), sig);
            f.start_script(thread, 0);
        } else {
            slide_to(f, s, s.goals[0], s.speed);
        }
    }
    if wait {
        for s in slides.iter().filter(|s| !s.shuttle) {
            wait_slide(f, s, s.speed);
        }
    }
    for (k, s) in motions.spins.iter().filter(|s| s.when == when).enumerate() {
        match s.sway {
            None => {
                for &(axis, speed, accel) in &s.axes {
                    f.push(accel).push(speed).spin(s.piece, axis);
                }
            }
            Some(_) => {
                let thread = sway(asm, s, &format!("{name}Sway{k}"), sig);
                f.start_script(thread, 0);
            }
        }
    }
}

/// What moves `when` brought to a stop in `f`: its threads answering `sig` ended, its spins slowed
/// to a stop at their acceleration (at once without one), its slides back where they were made.
fn stop_motions(f: &mut Function, motions: &Motions, when: When, sig: i32) {
    f.push(sig).op(op::SIGNAL);
    for s in motions.spins.iter().filter(|s| s.when == when) {
        for &(axis, _, accel) in &s.axes {
            f.push(accel).stop_spin(s.piece, axis);
        }
    }
    for s in motions.slides.iter().filter(|s| s.when == when && !s.goals.is_empty()) {
        slide_to(f, s, s.made, s.back);
    }
}

/// A swing's thread, `name`: back to rest, then round and round each way in turn, speeding up to
/// its speed and slowing to a stop again, a pause of some of its longest at random at each end.
fn sway(asm: &mut Assembler, s: &Spinning, name: &str, sig: i32) -> u32 {
    let (ms, pause) = s.sway.unwrap_or((1000, 0));
    let mut f = Function::new(0);
    f.push(sig).op(op::SET_SIGNAL_MASK);
    for &(axis, speed, _) in &s.axes {
        f.push(speed.saturating_abs()).push(0).turn(s.piece, axis);
    }
    for &(axis, _, _) in &s.axes {
        f.wait_turn(s.piece, axis);
    }
    let top = f.label();
    f.mark(top);
    for way in [1, -1] {
        for &(axis, speed, accel) in &s.axes {
            f.push(accel).push(speed.saturating_mul(way)).spin(s.piece, axis);
        }
        f.push(ms.max(1)).op(op::SLEEP);
        for &(axis, _, accel) in &s.axes {
            f.push(accel).stop_spin(s.piece, axis);
        }
        f.push(ms.max(1)).op(op::SLEEP);
        if pause > 0 {
            f.push(0).push(pause).op(op::RAND).op(op::SLEEP);
        }
    }
    f.jump(top);
    asm.define(name, f)
}

/// A shuttle's thread, `name`: to each of its places in turn, round and round (a pause where it
/// goes at once, so that it never runs on without one).
fn shuttle(asm: &mut Assembler, s: &Sliding, name: &str, sig: i32) -> u32 {
    let mut f = Function::new(0);
    f.push(sig).op(op::SET_SIGNAL_MASK);
    let top = f.label();
    f.mark(top);
    for &goal in &s.goals {
        slide_to(&mut f, s, goal, s.speed);
        wait_slide(&mut f, s, s.speed);
        if s.speed.iter().all(|&v| v == 0) {
            f.push(500).op(op::SLEEP);
        }
    }
    f.jump(top);
    asm.define(name, f)
}

/// The pieces hidden on a fresh unit: the bones its enhancements show and every bone under them
/// (a mesh's parents come before their children: one pass), in the model's order.
fn hidden_pieces(plan: &Plan) -> Vec<u32> {
    pieces_under(plan, &enhancement_bones(plan.unit))
}

/// The pieces of the bones `named` and every bone under them, in the model's order.
fn pieces_under(plan: &Plan, named: &[String]) -> Vec<u32> {
    let shown = named;
    let mut hidden = vec![false; plan.mesh.bones.len()];
    for (b, bone) in plan.mesh.bones.iter().enumerate() {
        hidden[b] = shown.iter().any(|s| s.eq_ignore_ascii_case(&bone.name)) || bone.parent.is_some_and(|p| hidden[p]);
    }
    let mut out: Vec<u32> = plan.mesh.bones.iter().enumerate().filter(|(b, _)| hidden[*b]).filter_map(|(_, bone)| plan.pieces.iter().position(|p| p.eq_ignore_ascii_case(&bone.name)).map(|i| i as u32)).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The bones a unit's enhancements show (`Enhancements.<name>.ShowBones`): hidden on a fresh unit.
pub fn enhancement_bones(unit: &UnitBlueprint) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(e) = unit.table.path(&["Enhancements"]) {
        for (_, value) in &e.fields {
            let Some(t) = value.table() else { continue };
            if let Some(bones) = t.get_ci("ShowBones").and_then(|v| v.table()) {
                for b in bones.array.iter().filter_map(|v| v.str()) {
                    if !out.iter().any(|o| o.eq_ignore_ascii_case(b)) {
                        out.push(b.to_string());
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scm;
    use crate::unit::{Rack, UnitBlueprint, WeaponBlueprint};

    fn gun() -> WeaponBlueprint {
        WeaponBlueprint {
            label: "MainGun".into(),
            turreted: true,
            yaw_bone: Some("Turret".into()),
            pitch_bone: Some("Turret_Barrel".into()),
            muzzle_bone: Some("Turret_Barrel".into()),
            racks: vec![Rack { rack: "Turret_Barrel".into(), muzzles: vec!["Turret_Barrel".into()] }],
            rack_recoil: -1.0,
            yaw_speed: 90.0,
            pitch_speed: 45.0,
            ..Default::default()
        }
    }

    /// The test tank's script: its gun aims, fires from its barrel, kicks back and rests again;
    /// it leaves a wreck.
    #[test]
    fn a_tanks_gun_is_aimed_and_fired() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let unit = UnitBlueprint { id: "TANK".into(), ..Default::default() };
        let w = gun();
        let plan = Plan { unit: &unit, mesh: &mesh, pieces: mesh.bones.iter().map(|b| b.name.clone()).collect(), weapons: vec![&w], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false };
        let s = script(&plan);
        for name in ["Create", "AimPrimary", "AimFromPrimary", "QueryPrimary", "FirePrimary", "RestoreAfterDelay", "Killed", "SetMaxReloadTime"] {
            assert!(s.function(name).is_some(), "{name}");
        }
        assert!(s.function("AimSecondary").is_none());
        let listing = ta_formats::cob::disassemble(&s);
        // The turret turned about y, the barrel about x (it rests ahead: no z turn).
        assert!(listing.contains("turn             1 1"), "{listing}");
        assert!(listing.contains("turn             2 0"), "{listing}");
        assert!(!listing.contains("turn             2 2"), "{listing}");
        // The script reads back as it was written.
        assert_eq!(CobScript::parse(&s.to_bytes()).unwrap().code, s.code);
    }

    /// A gun resting turned round: its heading taken off the aim's, and its pitch about z as
    /// well as x where it rests turned to a side.
    #[test]
    fn a_gun_resting_turned_aims_from_where_it_rests() {
        let still = [1.0, 0.0, 0.0, 0.0];
        // Half a turn about y (w, x, y, z), and a quarter turn.
        let behind = [0.0, 0.0, 1.0, 0.0];
        let side = [std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2, 0.0];
        let file = scm::tests::write(&[("Hull", -1, [0.0; 3], still), ("Rear", 0, [0.0, 1.0, -2.0], behind), ("Side", 0, [1.0, 1.0, 0.0], side)], &[([0.0; 3], [0.0; 2], 0)], &[[0, 0, 0]]);
        let mesh = scm::parse(&file).unwrap();
        let rear = rest_aim(&mesh, 1);
        assert_eq!((rear.heading.rem_euclid(65536), rear.cos, rear.sin), (32768, -1024, 0));
        let side_aim = rest_aim(&mesh, 2);
        assert_eq!((side_aim.heading, side_aim.cos, side_aim.sin), (16384, 0, 1024));
        let unit = UnitBlueprint { id: "SHIP".into(), ..Default::default() };
        let w = WeaponBlueprint { yaw_bone: Some("Side".into()), pitch_bone: Some("Side".into()), muzzle_bone: Some("Side".into()), racks: vec![], ..gun() };
        let plan = Plan { unit: &unit, mesh: &mesh, pieces: vec!["Hull".into(), "Rear".into(), "Side".into()], weapons: vec![&w], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false };
        let listing = ta_formats::cob::disassemble(&script(&plan));
        assert!(listing.contains("push-const       16384"), "{listing}");
        assert!(listing.contains("turn             2 2"), "the side gun pitches about z: {listing}");
    }

    /// Two guns on turrets of their own each turn back to rest by a restore of their own, which
    /// turns only their turret and answers their own aim alone; two sharing a turret both stop
    /// either's restore when they aim.
    #[test]
    fn each_weapon_turns_back_its_own_turret() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(&[("Hull", -1, [0.0; 3], still), ("Front", 0, [0.0, 1.0, 1.0], still), ("Back", 0, [0.0, 1.0, -1.0], still)], &[([0.0; 3], [0.0; 2], 0)], &[[0, 0, 0]]);
        let mesh = scm::parse(&file).unwrap();
        let unit = UnitBlueprint { id: "SHIP".into(), ..Default::default() };
        let on = |bone: &str| WeaponBlueprint { yaw_bone: Some(bone.into()), pitch_bone: None, muzzle_bone: Some(bone.into()), racks: vec![], ..gun() };
        let (front, back) = (on("Front"), on("Back"));
        let pieces: Vec<String> = vec!["Hull".into(), "Front".into(), "Back".into()];
        let body = |listing: &str, name: &str| -> String { listing.split(&format!("\n{name}:\n")).nth(1).map(|rest| rest.split("\n\n").next().unwrap_or("").to_string()).unwrap_or_default() };
        let mask = |body: &str| -> i32 { body.lines().next().and_then(|l| l.split_whitespace().last()).and_then(|v| v.parse().ok()).unwrap_or(-1) };
        let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![&front, &back], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false }));
        let (first, second) = (body(&listing, "RestoreAfterDelay"), body(&listing, "RestoreAfterDelay2"));
        assert!(first.contains("turn             1 1") && !first.contains("turn             2 1"), "{listing}");
        assert!(second.contains("turn             2 1") && !second.contains("turn             1 1"), "{listing}");
        assert_eq!((mask(&first), mask(&second)), (2, 4), "{listing}");
        // Sharing the front turret: either's aim stops both restores.
        let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces, weapons: vec![&front, &on("Front")], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false }));
        assert_eq!((mask(&body(&listing, "RestoreAfterDelay")), mask(&body(&listing, "RestoreAfterDelay2"))), (6, 6), "{listing}");
    }

    /// A fresh unit hides what its enhancements show and everything under it: the mount and the
    /// part on it.
    #[test]
    fn a_hidden_pieces_children_are_hidden_too() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(&[("Torso", -1, [0.0; 3], still), ("Back_Upgrade", 0, [0.0; 3], still), ("Back_Upgrade_B02", 1, [0.0; 3], still), ("Arm", 0, [0.0; 3], still)], &[([0.0; 3], [0.0; 2], 0)], &[[0, 0, 0]]);
        let mesh = scm::parse(&file).unwrap();
        let mut unit = UnitBlueprint { id: "ACU".into(), ..Default::default() };
        unit.table = crate::lua::parse("t = { Enhancements = { Shield = { ShowBones = { 'Back_Upgrade' } } } }").unwrap()[0].1.table().cloned().unwrap();
        let plan = Plan { unit: &unit, mesh: &mesh, pieces: mesh.bones.iter().map(|b| b.name.clone()).collect(), weapons: vec![], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false };
        assert_eq!(hidden_pieces(&plan), [1, 2]);
        let listing = ta_formats::cob::disassemble(&script(&plan));
        assert!(listing.contains("hide             1") && listing.contains("hide             2") && !listing.contains("hide             3"), "{listing}");
    }

    /// Two racks of a barrel each: the rack of the muzzle that fired kicks back (the muzzle counter
    /// names the muzzle, compared muzzle by muzzle), not the rack of the counter's number.
    #[test]
    fn the_rack_that_fired_kicks_back() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(
            &[("Hull", -1, [0.0; 3], still), ("Turret", 0, [0.0; 3], still), ("RackA", 1, [0.0; 3], still), ("MuzzleA1", 2, [0.0; 3], still), ("MuzzleA2", 2, [0.0; 3], still), ("RackB", 1, [0.0; 3], still), ("MuzzleB1", 5, [0.0; 3], still)],
            &[([0.0; 3], [0.0; 2], 0)],
            &[[0, 0, 0]],
        );
        let mesh = scm::parse(&file).unwrap();
        let unit = UnitBlueprint { id: "SHIP".into(), ..Default::default() };
        let w = WeaponBlueprint {
            yaw_bone: Some("Turret".into()),
            pitch_bone: None,
            racks: vec![Rack { rack: "RackA".into(), muzzles: vec!["MuzzleA1".into(), "MuzzleA2".into()] }, Rack { rack: "RackB".into(), muzzles: vec!["MuzzleB1".into()] }],
            ..gun()
        };
        let pieces: Vec<String> = mesh.bones.iter().map(|b| b.name.clone()).collect();
        let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces, weapons: vec![&w], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false }));
        let fire = listing.split("\nFirePrimary:\n").nth(1).unwrap().split("\n\n").next().unwrap();
        // Muzzles 0 and 1 on rack A (piece 2), muzzle 2 on rack B (piece 5).
        let moves: Vec<&str> = fire.lines().filter(|l| l.contains("move-now")).map(|l| l.split_whitespace().nth(2).unwrap_or("")).collect();
        assert_eq!(moves, ["2", "2", "2", "2", "5", "5"], "{fire}");
    }

    /// A commander whose gun turns its torso, the piece its build turns: while it builds, its aim
    /// declines and its restore leaves the torso to the build.
    #[test]
    fn a_commander_builds_without_its_gun_turning_its_torso() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let mut unit = UnitBlueprint { id: "ACU".into(), ..Default::default() };
        unit.build_bones = Some((Some("Turret".into()), None, vec!["Turret_Barrel".into()]));
        let w = gun();
        let pieces: Vec<String> = mesh.bones.iter().map(|b| b.name.clone()).collect();
        let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![&w], factory: false, builder: true, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false }));
        let body = |name: &str| listing.split(&format!("\n{name}:\n")).nth(1).unwrap().split("\n\n").next().unwrap().to_string();
        let (aim, restore, start, stop) = (body("AimPrimary"), body("RestoreAfterDelay"), body("StartBuilding"), body("StopBuilding"));
        // Aim: its first test the building flag, declining (0) while it is set.
        assert!(aim.lines().nth(2).is_some_and(|l| l.contains("push-static")) && aim.contains("jump-if-false"), "{aim}");
        assert!(restore.contains("push-static") && restore.contains("jump-if-false"), "{restore}");
        assert!(start.contains("pop-static") && stop.contains("pop-static"), "{start}\n{stop}");
        // A plain tank's aim has no such test.
        let plain = ta_formats::cob::disassemble(&script(&Plan { unit: &UnitBlueprint { id: "TANK".into(), ..Default::default() }, mesh: &mesh, pieces, weapons: vec![&w], factory: false, builder: false, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false }));
        let aim = plain.split("\nAimPrimary:\n").nth(1).unwrap().split("\n\n").next().unwrap();
        assert!(!aim.contains("jump-if-false 0") && !aim.lines().take(4).any(|l| l.contains("push-static")), "{aim}");
    }

    /// A stored missile leaving its tube upright from a muzzle resting under the ground (a silo's
    /// tube, opened and lifted in Forged Alliance) leaves from the silo's foot, its root: it would
    /// go off in its tube. One above the ground leaves from its muzzle.
    #[test]
    fn a_silos_missile_leaves_from_above_the_ground() {
        let still = [1.0, 0.0, 0.0, 0.0];
        let file = scm::tests::write(&[("Silo", -1, [0.0; 3], still), ("Deep", 0, [0.0, -40.0, 0.0], still), ("High", 0, [0.0, 10.0, 0.0], still)], &[([0.0; 3], [0.0; 2], 0)], &[[0, 0, 0]]);
        let mesh = scm::parse(&file).unwrap();
        let unit = UnitBlueprint { id: "SILO".into(), build_rate: 1080.0, ..Default::default() };
        let missile = |muzzle: &str| WeaponBlueprint {
            label: "Nuke".into(),
            category: "Missile".into(),
            counted: true,
            nuke: Some(crate::unit::Nuke { inner_damage: 1.0, inner_radius: 1.0, outer_damage: 1.0, outer_radius: 1.0 }),
            racks: vec![Rack { rack: muzzle.into(), muzzles: vec![muzzle.into()] }],
            ..Default::default()
        };
        let pieces: Vec<String> = mesh.bones.iter().map(|b| b.name.clone()).collect();
        let query = |w: &WeaponBlueprint| {
            let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![w], factory: false, builder: false, pixels: 1.0, enhancements: Vec::new(), motions: Motions::default(), switched: false }));
            listing.split("
QueryPrimary:
").nth(1).unwrap().lines().find(|l| l.contains("push-const")).unwrap().split_whitespace().last().unwrap().to_string()
        };
        assert_eq!(query(&missile("Deep")), "0", "the deep muzzle: from the root");
        assert_eq!(query(&missile("High")), "2", "the high one: from it");
    }

    /// The test tank's script with `motions`, played in the game's own script engine (ta_sim's
    /// `cob`) a tick at a time from its `Create`: `at` says what the engine tells it at a tick (its
    /// build done or not, a function to start), `look` sees its pieces after each.
    fn play(motions: Motions, switched: bool, factory: bool, ticks: i64, at: &mut dyn FnMut(i64) -> (bool, Option<&'static str>), look: &mut dyn FnMut(i64, &[ta_sim::cob::PieceAnim])) {
        use ta_sim::cob::{CobHost, CobInstance, CobProgram, ThreadTag};
        struct Host {
            now: i64,
            built: bool,
        }
        impl CobHost for Host {
            fn now_ms(&self) -> i64 {
                self.now
            }
            fn random(&mut self, lo: i32, hi: i32) -> i32 {
                (lo + hi) / 2
            }
            fn get_value(&mut self, _: &CobInstance, id: i32, _: [i32; 4]) -> i32 {
                if id == BUILD_PERCENT_LEFT && !self.built { 50 } else { 0 }
            }
            fn set_value(&mut self, _: i32, _: i32) {}
            fn explode(&mut self, _: &CobInstance, _: usize, _: i32) {}
            fn emit_sfx(&mut self, _: &CobInstance, _: usize, _: i32) {}
        }
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let mut unit = UnitBlueprint { id: "WORKS".into(), ..Default::default() };
        unit.display.build_attach_bone = Some("Turret".into());
        let pieces: Vec<String> = mesh.bones.iter().map(|b| b.name.clone()).collect();
        let s = script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![], factory, builder: factory, pixels: 2.0, enhancements: Vec::new(), motions, switched });
        let program = std::sync::Arc::new(CobProgram::new(CobScript::parse(&s.to_bytes()).unwrap(), &pieces));
        let mut cob = CobInstance::new(program);
        let mut host = Host { now: 0, built: false };
        cob.start("Create", &[], ThreadTag::None);
        for tick in 0..ticks {
            let (built, call) = at(tick);
            host.built = built;
            if let Some(f) = call {
                cob.start(f, &[], ThreadTag::None);
            }
            host.now = tick * 1000 / 30;
            cob.run(&mut host);
            cob.animate();
            look(tick, &cob.pieces);
        }
    }

    /// A building with no switch: nothing moves while it is built; once its build is done its
    /// exhaust slides out, then (the slide done) its turret spins up and turns on at its speed.
    #[test]
    fn a_generator_slides_out_and_spins_up_once_it_is_built() {
        let motions = Motions {
            spins: vec![Spinning { when: When::Working, piece: 1, axes: vec![(1, 16384, 182)], sway: None }],
            slides: vec![Sliding { when: When::Working, piece: 3, made: [0; 3], goals: vec![[0, 0, 262144]], shuttle: false, speed: [0, 0, 524288], back: [0, 0, 524288] }],
            left_out: Vec::new(),
        };
        let mut seen: Vec<(i32, i32)> = Vec::new();
        play(motions, false, false, 30 * 8, &mut |tick| (tick >= 30, None), &mut |_, p| seen.push((p[3].offset[2], p[1].rotation[1])));
        assert!(seen[..30].iter().all(|&s| s == (0, 0)), "still while it is built");
        let out = seen.iter().position(|s| s.0 == 262144).expect("slid out");
        assert!((40..=60).contains(&out), "out half a second after its build is done (a look a quarter second): {out}");
        assert_eq!(seen[out - 1].1, 0, "its spin waits for the slide");
        // Up to 90 degrees a second in three seconds, then on at that.
        let turned = |a: usize, b: usize| (seen[b].1 - seen[a].1).rem_euclid(65536);
        assert!(turned(out + 150, out + 180) > 16384 * 9 / 10, "a second at full speed: {}", turned(out + 150, out + 180));
        assert!(turned(out, out + 30) < 16384 / 4, "speeding up: {}", turned(out, out + 30));
    }

    /// A switched building: its dish swings out about its own axis and back while it is on, as
    /// Forged Alliance's do (speeding up one way and slowing to a stop, then the same the other
    /// way: out from where it rests and back to it, with no drift over the swings), and stops,
    /// switched off.
    #[test]
    fn a_dish_swings_while_switched_on_and_stops_when_switched_off() {
        let motions = Motions { spins: vec![Spinning { when: When::Working, piece: 2, axes: vec![(0, 2731, 30)], sway: Some((3000, 0)) }], ..Motions::default() };
        let mut seen: Vec<i32> = Vec::new();
        let mut on = |tick: i64| (true, match tick {
            0 => Some("Activate"),
            900 => Some("Deactivate"),
            _ => None,
        });
        play(motions, true, false, 1200, &mut on, &mut |_, p| seen.push(p[2].rotation[0] as i16 as i32));
        // 15 degrees a second, gathered over three seconds and lost over three: 45 degrees out.
        let (most, least) = (seen[..900].iter().copied().max().unwrap(), seen[..900].iter().copied().min().unwrap());
        assert!((most - 8192).abs() < 300 && least.abs() < 300, "{least}..{most}");
        assert!(seen[715..725].iter().any(|v| v.abs() < 200), "back where it started after a whole swing each way: {:?}", &seen[715..725]);
        assert_eq!(seen[1100], seen[1199], "stopped, switched off");
    }

    /// A factory's arm shuttles between its places while it builds and goes back to where it
    /// rests once it stops.
    #[test]
    fn a_factorys_arm_shuttles_while_it_builds() {
        let motions = Motions {
            slides: vec![Sliding { when: When::Building, piece: 3, made: [0; 3], goals: vec![[0, 131072, 0], [0, -131072, 0]], shuttle: true, speed: [0, 262144, 0], back: [0, 524288, 0] }],
            ..Motions::default()
        };
        let mut seen: Vec<i32> = Vec::new();
        play(motions, false, true, 300, &mut |tick| (true, match tick {
            0 => Some("StartBuilding"),
            200 => Some("StopBuilding"),
            _ => None,
        }), &mut |_, p| seen.push(p[3].offset[1]));
        assert_eq!((seen[..200].iter().copied().max(), seen[..200].iter().copied().min()), (Some(131072), Some(-131072)), "up and down");
        assert_eq!(seen[299], 0, "back at rest");
    }

    /// What spins from a unit's making spins from its `Create`, even while it is built (the
    /// Seraphim factories' pods); a part slid aside as it is made is put there at once.
    #[test]
    fn what_moves_from_the_making_moves_at_once() {
        let motions = Motions {
            spins: vec![Spinning { when: When::Made, piece: 1, axes: vec![(1, 910, 0)], sway: None }],
            slides: vec![Sliding { when: When::Working, piece: 3, made: [0, -65536, 0], goals: vec![[0; 3]], shuttle: false, speed: [0, 65536, 0], back: [0, 65536, 0] }],
            left_out: Vec::new(),
        };
        let mut seen: Vec<(i32, i32)> = Vec::new();
        play(motions, true, false, 60, &mut |_| (false, None), &mut |_, p| seen.push((p[1].rotation[1], p[3].offset[1])));
        assert_eq!(seen[0].1, -65536, "set aside at once");
        assert!((seen[59].0 - 910 * 59 / 30).abs() <= 30, "turning from the start: {}", seen[59].0);
        assert_eq!(seen[59].1, -65536, "it waits for its switch to slide back");
    }

    /// An engineer lifts its stance to build and sprays from its effect bones in turn; a factory
    /// opens on Activate and builds on its attach bone.
    #[test]
    fn builders_build_and_factories_open() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let mut unit = UnitBlueprint { id: "ENGINEER".into(), ..Default::default() };
        unit.build_bones = Some((Some("Turret".into()), None, vec!["Turret_Barrel".into(), "Exhaust".into()]));
        let pieces: Vec<String> = mesh.bones.iter().map(|b| b.name.clone()).collect();
        let s = script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![], factory: false, builder: true, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false });
        assert!(s.function("StartBuilding").is_some() && s.function("QueryNanoPiece").is_some() && s.function("Activate").is_none());
        let listing = ta_formats::cob::disassemble(&s);
        assert!(listing.lines().any(|l| l.trim_end().ends_with(" set")), "the stance set: {listing}");
        unit.display.build_attach_bone = Some("Turret".into());
        let s = script(&Plan { unit: &unit, mesh: &mesh, pieces, weapons: vec![], factory: true, builder: true, pixels: 2.0, enhancements: Vec::new(), motions: Motions::default(), switched: false });
        assert!(s.function("Activate").is_some() && s.function("QueryBuildInfo").is_some());
    }
}
