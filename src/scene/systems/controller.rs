use glam::{Quat, Vec3};
use hecs::World;

use crate::{
    input::{Input, InputAction},
    scene::components::{Camera, Controller, Transform},
};

const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

pub fn update(world: &mut World, input: &Input, delta_seconds: f32) {
    if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
        return;
    }

    let delta_seconds = delta_seconds.min(0.1);

    let query = world.query_mut::<(&Camera, &mut Controller, &mut Transform)>();

    for (_, controller, transform) in query {
        update_camera(controller, transform, input, delta_seconds);
    }
}

fn update_camera(
    controller: &mut Controller,
    transform: &mut Transform,
    input: &Input,
    delta_seconds: f32,
) {
    if input.action_pressed(InputAction::Look) {
        let mouse_delta = input.mouse_delta();

        controller.yaw = (controller.yaw - mouse_delta.x * controller.look_sensitivity)
            .rem_euclid(std::f32::consts::TAU);

        controller.pitch = (controller.pitch - mouse_delta.y * controller.look_sensitivity)
            .clamp(-MAX_PITCH, MAX_PITCH);

        transform.rotation =
            Quat::from_rotation_y(controller.yaw) * Quat::from_rotation_x(controller.pitch);
    }

    let forward = transform.rotation * Vec3::NEG_Z;
    let right = transform.rotation * Vec3::X;
    let direction = right * input.axis(InputAction::MoveLeft, InputAction::MoveRight)
        + Vec3::Y * input.axis(InputAction::MoveDown, InputAction::MoveUp)
        + forward * input.axis(InputAction::MoveBackward, InputAction::MoveForward);

    transform.translation += direction.normalize_or_zero() * controller.move_speed * delta_seconds;
}
