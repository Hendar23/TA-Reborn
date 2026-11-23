use super::*;
use crate::unit::{DirectControl, DirectInput};

impl Sim {
    pub(super) fn set_direct_control(&mut self, player: u8, id: UnitId, input: Option<DirectInput>) {
        let Some(unit) = self.unit(id) else { return };
        let ty = self.types.get(unit.type_id);
        if unit.owner != player || unit.dead || !unit.is_complete() || unit.carried_by.is_some()
            || !matches!(ty.locomotion, Locomotion::Ground | Locomotion::Hover | Locomotion::Naval) {
            return;
        }
        if input.is_some_and(|i| !(-1..=1).contains(&i.throttle) || !(-1..=1).contains(&i.turn) || !(-32768..=32767).contains(&i.heading) || !(-16384..=16384).contains(&i.pitch)) {
            return;
        }
        let entering = unit.direct_control.is_none();
        if entering && input.is_none() { return; }
        let aim_piece = unit.direct_control.and_then(|control| control.aim_piece);
        if entering || input.is_none() { self.halt(id); }
        let expires = self.tick + 90;
        if let Some(unit) = self.unit_mut(id) {
            unit.direct_control = input.map(|input| DirectControl { player, input, expires, aim_piece });
            if entering || input.is_none() {
                for slot in 0..ta_data::unit::MAX_WEAPONS { unit.retarget(slot, None); }
            }
        }
    }

    pub(super) fn direct_step(unit: &mut Unit, ty: &UnitType, spec: MoveSpec, world: &World, input: DirectInput) {
        unit.heading = wrap_angle(unit.heading - input.turn as i32 * ty.turn_rate);
        let target = ty.max_speed * input.throttle as i64;
        let acceleration = if unit.speed.signum() != target.signum() || target.abs() < unit.speed.abs() { ty.brake } else { ty.acceleration };
        unit.speed = unit.speed.approach(target, acceleration);
        let step = heading_vector(unit.heading).scale(unit.speed);
        let candidates = [unit.pos + step, unit.pos + FxVec3::new(step.x, Fx::ZERO, Fx::ZERO), unit.pos + FxVec3::new(Fx::ZERO, Fx::ZERO, step.z)];
        let ignore = unit.factory.map(|factory| factory.slot);
        let current_ok = world.passable(&spec, World::pos_to_anchor(unit.pos, ty.footprint), ignore);
        if let Some(position) = candidates.into_iter().find(|&point| world.ground_segment_open(&spec, unit.pos, point, ignore, !current_ok)) {
            unit.pos.x = position.x;
            unit.pos.z = position.z;
        } else { unit.speed = Fx::ZERO; }
        let moving = !unit.speed.is_zero();
        if moving != unit.moving {
            unit.moving = moving;
            if let Some(script) = &mut unit.cob {
                script.start(if moving { "StartMoving" } else { "StopMoving" }, &[], ThreadTag::None);
            }
        }
        Self::settle(unit, ty, world);
    }
}
