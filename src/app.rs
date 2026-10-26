use std::sync::Arc;

use anyhow::{Context, Result};
use glam::Vec2;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, StartCause, WindowEvent},
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
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            event_loop.set_control_flow(ControlFlow::Wait);
            if let Some(state) = &self.state {
                state.request_redraw();
            }
        }
    }

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

        let ui_response = state.handle_window_event(&event);

        let button_event = match &event {
            WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
                PhysicalKey::Code(key) => Some((InputButton::Key(key), event.state)),
                PhysicalKey::Unidentified(_) => None,
            },
            WindowEvent::MouseInput { button, state, .. } => {
                Some((InputButton::Mouse(*button), *state))
            }
            _ => None,
        };

        if let Some((button, button_state)) = button_event
            && (!ui_response.consumed || button_state == ElementState::Released)
        {
            state.set_input_button(button, button_state == ElementState::Pressed);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) => state.clear_input(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::RedrawRequested => {
                state.update();
                match state.render() {
                    Ok(Some(response)) => {
                        if response.quit_requested {
                            event_loop.exit();
                        } else if response.repaint_delay.is_zero() {
                            event_loop.set_control_flow(ControlFlow::Wait);
                            state.request_redraw();
                        } else {
                            event_loop.set_control_flow(ControlFlow::wait_duration(
                                response.repaint_delay,
                            ));
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.error = Some(error.context("failed to render frame"));
                        event_loop.exit();
                    }
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
