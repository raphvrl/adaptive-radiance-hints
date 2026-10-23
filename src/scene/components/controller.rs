#[derive(Clone, Copy, Debug)]
pub struct Controller {
    pub move_speed: f32,
    pub look_sensitivity: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            move_speed: 4.0,
            look_sensitivity: 0.002,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}
