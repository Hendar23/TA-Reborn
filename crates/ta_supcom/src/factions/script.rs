//! A unit's script (`scripts/<id>.cob`), made from its blueprint and its mesh: TA's engine calls
//! the same functions of every unit's script (docs/FORMATS.md, "COB bytecode"), and these answer
//! them as the unit's bones say.
//!
//! - **Weapons** (TA's three slots: `AimPrimary`... `FireTertiary`): a turret turns its yaw bone to
//!   the target's heading less the heading it rests at, and pitches its pitch bone about the axis
//!   square to the way it rests (an x turn and a z turn, `cos` and `sin` of that heading made
//!   numbers in the script: exact for guns resting ahead, behind or to a side, close for the
//!   rest), less the pitch it rests at; the model is the mesh at rest, so a rear or side gun stands
//!   as it should wherever no script turns it (wrecks, pictures). The muzzles of its racks take
//!   turns, the rack kicking back along its barrel; twice the longest reload after its last aim, it
//!   turns back to rest. A weapon without a turret is always aimed.
//! - **Builders** lift their build stance on `StartBuilding` (an engineer turning its build bones
//!   to the work), and the spray leaves from their build effect bones in turn. Factories open on
//!   `Activate` and build on the piece their blueprint attaches units to.
//! - A unit leaves a wreck (`Killed`); a fresh one hides the pieces its enhancements show.

use ta_formats::cob::{Assembler, CobScript, Function, X_AXIS, Y_AXIS, Z_AXIS, op};

use crate::convert::trig::atan2;
use crate::scm::{self, Mesh};
use crate::unit::{UnitBlueprint, WeaponBlueprint};

/// TA's names of a weapon slot's functions.
const AIM: [&str; 3] = ["AimPrimary", "AimSecondary", "AimTertiary"];
const AIM_FROM: [&str; 3] = ["AimFromPrimary", "AimFromSecondary", "AimFromTertiary"];
const QUERY: [&str; 3] = ["QueryPrimary", "QuerySecondary", "QueryTertiary"];
const FIRE: [&str; 3] = ["FirePrimary", "FireSecondary", "FireTertiary"];

/// The unit values scripts set (ta_sim's `host::value`).
const IN_BUILD_STANCE: i32 = 5;
const YARD_OPEN: i32 = 18;

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

/// Each weapon slot's restore, turning its turrets back to rest.
const RESTORE: [&str; 3] = ["RestoreAfterDelay", "RestoreAfterDelay2", "RestoreAfterDelay3"];

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
    let mut rests: [Vec<(u32, u32, i32)>; 3] = Default::default();

    // Create: hidden what the enhancements show; a reload of a second until told.
    let mut create = Function::new(0);
    create.push(1000).pop_static(reload);
    for shown in enhancement_bones(plan.unit) {
        if let Some(p) = piece(&shown) {
            create.hide(p);
        }
    }
    create.ret(0);
    asm.define("Create", create);
    let mut set_reload = Function::new(1);
    set_reload.push_local(0).pop_static(reload).ret(0);
    asm.define("SetMaxReloadTime", set_reload);

    for (slot, w) in plan.weapons.iter().enumerate().take(3) {
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
        let aim_from = yaw.map(|(p, _)| p).or(pitch.map(|(p, _)| p)).or(muzzles.first().map(|m| m.0)).unwrap_or(0);
        let next = asm.static_var();

        if w.turreted && (yaw.is_some() || pitch.is_some()) {
            let rest = yaw.map(|(_, b)| rest_aim(plan.mesh, b)).unwrap_or(RestAim { heading: 0, cos: ONE as i32, sin: 0, pitch: 0 });
            let barrel = pitch.map(|(_, b)| rest_aim(plan.mesh, b)).unwrap_or(rest);
            let (yaw_speed, pitch_speed) = (angle_speed(w.yaw_speed), angle_speed(w.pitch_speed));
            let mut aim = Function::new(2);
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
            let restore = asm.declare(RESTORE[slot]);
            aim.start_script(restore, 0).ret(1);
            asm.define(AIM[slot], aim);
        }

        let mut from = Function::new(1);
        from.push(aim_from as i32).pop_local(0).ret(0);
        asm.define(AIM_FROM[slot], from);

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
        asm.define(QUERY[slot], query);

        // Fire: the rack kicks back along its barrel and returns; the next muzzle's turn.
        let mut fire = Function::new(0);
        let kick = (w.rack_recoil.abs() * plan.pixels).min(8.0);
        if kick > 0.1 {
            let rest = pitch.or(yaw).map(|(_, b)| rest_aim(plan.mesh, b)).unwrap_or(RestAim { heading: 0, cos: ONE as i32, sin: 0, pitch: 0 });
            let racks: Vec<u32> = {
                let mut r: Vec<u32> = muzzles.iter().filter_map(|m| m.1).collect();
                r.dedup();
                r
            };
            for (k, rack) in racks.iter().enumerate() {
                let skip = fire.label();
                if racks.len() > 1 {
                    fire.push_static(next).push(k as i32).op(op::SET_EQUAL).jump_if_false(skip);
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
        asm.define(FIRE[slot], fire);
    }

    // Each weapon's turrets back to rest twice the longest reload after its last aim. Its restore
    // answers the signal of every weapon turning any of its pieces, so an aim of any of them stops
    // it: a weapon gone quiet never turns back a turret another is aiming.
    for (slot, own) in rests.iter().enumerate().filter(|(_, own)| !own.is_empty()) {
        let mask = rests.iter().enumerate().filter(|(_, other)| other.iter().any(|o| own.iter().any(|s| s.0 == o.0))).fold(0i32, |mask, (j, _)| mask | (2 << j));
        let mut back = Function::new(0);
        back.push(mask).op(op::SET_SIGNAL_MASK);
        back.push_static(reload).push(2).op(op::MUL).op(op::SLEEP);
        for &(p, axis, speed) in own {
            back.push(speed).push(0).turn(p, axis);
        }
        back.ret(0);
        asm.define(RESTORE[slot], back);
    }

    if plan.builder {
        build_functions(&mut asm, plan);
    }

    // A wreck where it dies.
    let mut killed = Function::new(2);
    killed.push(1).pop_local(1).ret(0);
    asm.define("Killed", killed);
    asm.finish()
}

/// The build functions: the stance, the spray's pieces, a factory's opening and pad.
fn build_functions(asm: &mut Assembler, plan: &Plan) {
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
        start.ret(0);
        asm.define("StartBuilding", start);
        let mut stop = Function::new(0);
        stop.ret(0);
        asm.define("StopBuilding", stop);
    } else {
        let yaw = yaw_bone.as_deref().and_then(|b| piece(b).zip(plan.mesh.bone(b)));
        let pitch = pitch_bone.as_deref().and_then(|b| piece(b).zip(plan.mesh.bone(b)));
        let mut start = Function::new(2);
        if let Some((y, b)) = yaw {
            let rest = rest_aim(plan.mesh, b);
            start.push(angle_speed(180.0)).push_local(0).push(rest.heading).op(op::SUB).turn(y, Y_AXIS).wait_turn(y, Y_AXIS);
        }
        if let Some((p, _)) = pitch {
            start.push(angle_speed(180.0)).push(0).turn(p, X_AXIS);
        }
        set(&mut start, IN_BUILD_STANCE, 1);
        start.ret(0);
        asm.define("StartBuilding", start);
        let mut stop = Function::new(0);
        set(&mut stop, IN_BUILD_STANCE, 0);
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
        let plan = Plan { unit: &unit, mesh: &mesh, pieces: mesh.bones.iter().map(|b| b.name.clone()).collect(), weapons: vec![&w], factory: false, builder: false, pixels: 2.0 };
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
        let plan = Plan { unit: &unit, mesh: &mesh, pieces: vec!["Hull".into(), "Rear".into(), "Side".into()], weapons: vec![&w], factory: false, builder: false, pixels: 2.0 };
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
        let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![&front, &back], factory: false, builder: false, pixels: 2.0 }));
        let (first, second) = (body(&listing, "RestoreAfterDelay"), body(&listing, "RestoreAfterDelay2"));
        assert!(first.contains("turn             1 1") && !first.contains("turn             2 1"), "{listing}");
        assert!(second.contains("turn             2 1") && !second.contains("turn             1 1"), "{listing}");
        assert_eq!((mask(&first), mask(&second)), (2, 4), "{listing}");
        // Sharing the front turret: either's aim stops both restores.
        let listing = ta_formats::cob::disassemble(&script(&Plan { unit: &unit, mesh: &mesh, pieces, weapons: vec![&front, &on("Front")], factory: false, builder: false, pixels: 2.0 }));
        assert_eq!((mask(&body(&listing, "RestoreAfterDelay")), mask(&body(&listing, "RestoreAfterDelay2"))), (6, 6), "{listing}");
    }

    /// An engineer lifts its stance to build and sprays from its effect bones in turn; a factory
    /// opens on Activate and builds on its attach bone.
    #[test]
    fn builders_build_and_factories_open() {
        let mesh = scm::parse(&scm::tests::tank()).unwrap();
        let mut unit = UnitBlueprint { id: "ENGINEER".into(), ..Default::default() };
        unit.build_bones = Some((Some("Turret".into()), None, vec!["Turret_Barrel".into(), "Exhaust".into()]));
        let pieces: Vec<String> = mesh.bones.iter().map(|b| b.name.clone()).collect();
        let s = script(&Plan { unit: &unit, mesh: &mesh, pieces: pieces.clone(), weapons: vec![], factory: false, builder: true, pixels: 2.0 });
        assert!(s.function("StartBuilding").is_some() && s.function("QueryNanoPiece").is_some() && s.function("Activate").is_none());
        let listing = ta_formats::cob::disassemble(&s);
        assert!(listing.lines().any(|l| l.trim_end().ends_with(" set")), "the stance set: {listing}");
        unit.display.build_attach_bone = Some("Turret".into());
        let s = script(&Plan { unit: &unit, mesh: &mesh, pieces, weapons: vec![], factory: true, builder: true, pixels: 2.0 });
        assert!(s.function("Activate").is_some() && s.function("QueryBuildInfo").is_some());
    }
}
