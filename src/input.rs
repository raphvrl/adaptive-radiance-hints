use std::collections::HashSet;

use glam::Vec2;
use winit::{event::MouseButton, keyboard::KeyCode};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputAction {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    Look,
}

impl InputAction {
    fn buttons(self) -> &'static [InputButton] {
        match self {
            Self::MoveForward => &[
                InputButton::Key(KeyCode::KeyW),
                InputButton::Key(KeyCode::ArrowUp),
            ],
            Self::MoveBackward => &[
                InputButton::Key(KeyCode::KeyS),
                InputButton::Key(KeyCode::ArrowDown),
            ],
            Self::MoveLeft => &[
                InputButton::Key(KeyCode::KeyA),
                InputButton::Key(KeyCode::ArrowLeft),
            ],
            Self::MoveRight => &[
                InputButton::Key(KeyCode::KeyD),
                InputButton::Key(KeyCode::ArrowRight),
            ],
            Self::MoveUp => &[InputButton::Key(KeyCode::Space)],
            Self::MoveDown => &[InputButton::Key(KeyCode::ShiftLeft)],
            Self::Look => &[InputButton::Mouse(MouseButton::Right)],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputButton {
    Key(KeyCode),
    Mouse(MouseButton),
}

#[derive(Default)]
pub struct Input {
    down_buttons: HashSet<InputButton>,
    mouse_delta: Vec2,
}

impl Input {
    pub fn set_button(&mut self, button: InputButton, pressed: bool) {
        if pressed {
            self.down_buttons.insert(button);
        } else {
            self.down_buttons.remove(&button);
        }
    }

    pub fn action_pressed(&self, action: InputAction) -> bool {
        action
            .buttons()
            .iter()
            .any(|button| self.down_buttons.contains(button))
    }

    pub fn axis(&self, negative: InputAction, positive: InputAction) -> f32 {
        f32::from(self.action_pressed(positive)) - f32::from(self.action_pressed(negative))
    }

    pub fn has_active_actions(&self) -> bool {
        [
            InputAction::MoveForward,
            InputAction::MoveBackward,
            InputAction::MoveLeft,
            InputAction::MoveRight,
            InputAction::MoveUp,
            InputAction::MoveDown,
            InputAction::Look,
        ]
        .into_iter()
        .any(|action| self.action_pressed(action))
    }

    pub fn add_mouse_delta(&mut self, delta: Vec2) {
        self.mouse_delta += delta;
    }

    pub fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }

    pub fn end_frame(&mut self) {
        self.mouse_delta = Vec2::ZERO;
    }

    pub fn clear(&mut self) {
        self.down_buttons.clear();
        self.end_frame();
    }
}
