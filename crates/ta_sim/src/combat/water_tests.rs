//! What TA's own units hit in and under the water: torpedoes everything that reaches into it,
//! surface weapons nothing wholly under it. They skip without an install.

use super::targeting_tests::{put_at, tas_battle};
use super::*;

/// Whether `shooter` fires within `ticks`, and whether `victim` loses health meanwhile (or dies).
fn fires_and_hurts(sim: &mut Sim, shooter: UnitId, victim: UnitId, ticks: u32) -> (bool, bool) {
    let start = sim.unit(victim).unwrap().health;
    let mut fired = false;
    for _ in 0..ticks {
        sim.step(&[]);
        fired |= sim.take_events().iter().any(|e| matches!(e, SimEvent::WeaponFired { unit, .. } if *unit == shooter));
        if fired && sim.unit(victim).is_none_or(|u| u.health < start) {
            break;
        }
    }
    (fired, sim.unit(victim).is_none_or(|u| u.health < start))
}

fn calm(sim: &mut Sim, id: UnitId) {
    let u = sim.unit_mut(id).unwrap();
    u.fire_state = 0;
    u.move_state = 0;
}

/// TA's submarines torpedo what floats on the water as well as what is under it: the Lurker and
/// the Snake each hit the other side's floating missile tower, the Defender - NS (0.3 px into the
/// water, 61 tall) and the Stinger (4 into it, 53 tall), whose collision spheres stand over the
/// water, out of the way of the torpedoes run under them; in deep water and on a shelf 10 and 3 px
/// deep, from 200 px and from 360 (past the 300 px their torpedoes' `weapontimer`, 3 s at 100 px
/// a second, ended their run at, short of their 400 px range). Destroyers and submarines are hit
/// as they were.
#[test]
fn submarines_torpedo_the_floating_missile_towers() {
    if tas_battle(150, 60).is_none() {
        eprintln!("TA install not found; skipping");
        return;
    }
    let mut misses = Vec::new();
    for (sub, targets) in [("ARMSUB", ["CORFRT", "CORROY", "CORSUB"]), ("CORSUB", ["ARMFRT", "ARMROY", "ARMSUB"])] {
        for target in targets {
            let towers = target.ends_with("FRT");
            let shelves: &[u8] = if towers { &[60, 140, 147] } else { &[60] };
            for &shelf in shelves {
                for distance in [200, 360] {
                    // (Sea level 150 over ground at 60, rising to `shelf` past x = 1,100.)
                    let mut sim = tas_battle(150, shelf).unwrap();
                    sim.los_enabled = false;
                    let x = if towers { 1300 } else { 1000 };
                    let victim = put_at(&mut sim, target, 1, x, 1000);
                    calm(&mut sim, victim);
                    let shooter = put_at(&mut sim, sub, 0, x - distance, 1000);
                    let (fired, hit) = fires_and_hurts(&mut sim, shooter, victim, 600);
                    if !(fired && hit) {
                        let depth = sim.world.depth_at(Fx::from_int(x), Fx::from_int(1000)).to_int();
                        misses.push(format!("{sub} on {target} in {depth} px of water from {distance} px: fired {fired}, hit {hit}"));
                    }
                }
            }
        }
    }
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

/// A destroyer sent at a submarine 600 px off (in its guns' reach, 660, past its depth charges',
/// 400 and 410) closes to its depth charges' reach, its guns reaching nothing under the water,
/// and hits it: it stopped at its guns' reach and never fired.
#[test]
fn a_destroyer_sent_at_a_submarine_closes_to_its_depth_charges_reach() {
    if tas_battle(150, 60).is_none() {
        eprintln!("TA install not found; skipping");
        return;
    }
    for (destroyer, sub) in [("CORROY", "ARMSUB"), ("ARMROY", "CORSUB")] {
        let mut sim = tas_battle(150, 60).unwrap();
        sim.los_enabled = false;
        let ship = put_at(&mut sim, destroyer, 0, 500, 1000);
        let target = put_at(&mut sim, sub, 1, 1100, 1000);
        calm(&mut sim, target);
        sim.step(&[crate::Command::Order { player: 0, units: vec![ship], order: Order::Attack(Target::Unit(target)), queue: false }]);
        let (fired, hit) = fires_and_hurts(&mut sim, ship, target, 900);
        let at = sim.unit(ship).unwrap().pos.x.to_int();
        assert!(fired && hit, "{destroyer} on {sub}: fired {fired}, hit {hit}, the destroyer at x = {at}");
    }
}

/// TA's underwater buildings are wholly under the water however tall their models: the Skeeter
/// never fires on an underwater metal extractor, the Arm's (42 px tall) in 25 px of water or the
/// Core's (19 px) in 12, of its own accord or by an order, which ends; a submarine sinks them. A
/// Core amphibious K-bot (32 px tall) in 20 px of water is partly out of it, and both hit it; in
/// the deep, only the submarine.
#[test]
fn surface_weapons_never_take_on_tas_underwater_buildings() {
    if tas_battle(150, 60).is_none() {
        eprintln!("TA install not found; skipping");
        return;
    }
    // (target, the shelf's height under it: the water's depth there is 150 less that.)
    let cases = [("ARMUWMEX", 125, true), ("CORUWMEX", 138, true), ("CORAMPH", 130, false), ("CORAMPH", 60, true)];
    for (target, shelf, under) in cases {
        let x = if shelf == 60 { 900 } else { 1300 };
        let mut sim = tas_battle(150, shelf).unwrap();
        sim.los_enabled = false;
        let victim = put_at(&mut sim, target, 1, x, 1000);
        calm(&mut sim, victim);
        let v = sim.unit(victim).unwrap();
        let depth = sim.world.depth_at(v.pos.x, v.pos.z).to_int();
        assert!(sim.in_water(v), "{target} stands in {depth} px of water");
        assert_eq!(sim.submerged(v), under, "{target} in {depth} px of water, {} tall: wholly under it {under}", sim.unit_type(v).model.height.to_int());
        if target.ends_with("UWMEX") {
            assert!(v.pos.y + sim.unit_type(v).model.height > sim.world.sea_level, "{target}'s model reaches out of {depth} px of water");
        }
        // The Skeeter, 150 px off (in its laser's reach), at will and then ordered.
        let skeeter = put_at(&mut sim, "ARMPT", 0, x - 150, 1000);
        let at_will = fires_and_hurts(&mut sim, skeeter, victim, 150);
        assert_eq!(at_will, (!under, !under), "the Skeeter at will on {target} in {depth} px of water");
        if under {
            sim.step(&[crate::Command::Order { player: 0, units: vec![skeeter], order: Order::Attack(Target::Unit(victim)), queue: false }]);
            assert_eq!(fires_and_hurts(&mut sim, skeeter, victim, 90), (false, false), "nor by an order on {target}");
            assert!(sim.unit(skeeter).unwrap().orders.is_empty(), "the order on {target} ends");
        }
        // A submarine in the deep, 250 px off.
        let mut sim = tas_battle(150, shelf).unwrap();
        sim.los_enabled = false;
        let victim = put_at(&mut sim, target, 1, x, 1000);
        calm(&mut sim, victim);
        let sub = put_at(&mut sim, "ARMSUB", 0, x - 250, 1000);
        assert_eq!(fires_and_hurts(&mut sim, sub, victim, 600), (true, true), "the Lurker torpedoes {target} in {depth} px of water");
    }
}
