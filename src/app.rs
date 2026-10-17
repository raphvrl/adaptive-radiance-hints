use std::sync::Arc;

use anyhow::{Context, Result};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use crate::state::State;

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
                state.window().request_redraw();
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

        if state.window().id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.resize(size);
                state.window().request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = state.render() {
                    self.error = Some(error.context("failed to render frame"));
                    event_loop.exit();
                }
            }
            _ => {}
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
