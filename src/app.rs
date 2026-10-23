use std::sync::Arc;

use anyhow::{Context, Result};
use glam::Vec2;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop},
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

use crate::{input::InputButton, state::State};

#[derive(Default)]
pub struct App {
    state: Option<State>,
    error: Option<anyhow::Error>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() || self.error.is_some() {
            return;
        }

        event_loop.listen_device_events(DeviceEvents::WhenFocused);

        let window_attributes = Window::default_attributes().with_title("Adaptive Radiance Hints");

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.error = Some(anyhow::Error::new(error).context("failed to create window"));
                event_loop.exit();
                return;
            }
        };

        match pollster::block_on(State::new(window)) {
            Ok(state) => {
                state.request_redraw();
                self.state = Some(state);
            }
            Err(error) => {
                self.error = Some(error.context("failed to initialize graphics"));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        if state.window_id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => state.clear_input(),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    state.set_input_button(
                        InputButton::Key(key),
                        event.state == ElementState::Pressed,
                    );
                }
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                state.set_input_button(
                    InputButton::Mouse(button),
                    button_state == ElementState::Pressed,
                );
            }
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::RedrawRequested => {
                state.update();
                if let Err(error) = state.render() {
                    self.error = Some(error.context("failed to render frame"));
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            state.add_mouse_delta(Vec2::new(dx as f32, dy as f32));
        }
    }
}

pub fn run() -> Result<()> {
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();

    event_loop.run_app(&mut app).context("event loop failed")?;

    if let Some(error) = app.error {
        return Err(error);
    }

    Ok(())
}
