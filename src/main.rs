//! vxwm - minimal experimental Wayland compositor (winit-backend prototype).
//!
//! Architecture (mirrors Smithay's `smallvil` reference, trimmed to the spec):
//!   - `state`    : the `Vxwm` struct owning all protocol + desktop state.
//!   - `handlers` : trait impls Smithay dispatches client requests through.
//!   - `winit`    : opens a window in the host session and renders the space.
//!   - `input`    : turns backend input into seat events + the exit keybind.
//!
//! Run it from inside an existing Wayland or X11 session. It opens a black
//! window that is a fully functional Wayland server: point other clients at
//! the `WAYLAND_DISPLAY` it prints and they will render into it. Press
//! `Super+Escape` (or close the window) to quit.

mod handlers;
mod input;
mod state;
mod winit;

pub use state::Vxwm;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};

/// Data threaded through every calloop callback: the compositor state plus a
/// handle to the Wayland display for flushing clients.
pub struct CalloopData {
    pub state: Vxwm,
    pub display_handle: DisplayHandle,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging. Respect `RUST_LOG` if set, otherwise default to `info`.
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
            .init();
    }

    // The event loop drives everything: socket, clients, input and rendering.
    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;

    // The Wayland display owns the protocol object graph.
    let display: Display<Vxwm> = Display::new()?;
    let display_handle = display.handle();

    let state = Vxwm::new(&mut event_loop, display);
    let mut data = CalloopData {
        state,
        display_handle,
    };

    // Bring up the winit window/renderer and register its event source.
    winit::init_winit(&mut event_loop, &mut data)?;

    tracing::info!("vxwm running; press Super+Escape or close the window to exit");

    // Block here until `loop_signal.stop()` is called (close request / keybind).
    event_loop.run(None, &mut data, |data| {
        // Keep clients flushed between dispatch cycles.
        data.state.space.refresh();
        data.state.popups.cleanup();
        let _ = data.display_handle.flush_clients();
    })?;

    tracing::info!("vxwm exited cleanly");
    Ok(())
}
