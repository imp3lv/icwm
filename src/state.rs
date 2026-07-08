//! Compositor state.
//!
//! `Vxwm` owns every piece of Smithay protocol state plus a `Space` (the 2D
//! plane windows/outputs are mapped onto). It is the single value threaded
//! through the calloop event loop.

use std::{ffi::OsString, sync::Arc};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    reexports::{
        calloop::{generic::Generic, EventLoop, Interest, LoopSignal, Mode, PostAction},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle,
        },
    },
    utils::{Logical, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::XdgShellState,
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::CalloopData;

/// Top-level compositor state.
pub struct Vxwm {
    /// Monotonic clock start, used to timestamp frame callbacks.
    pub start_time: std::time::Instant,
    /// Name of the Wayland socket clients connect to (e.g. `wayland-1`).
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    /// The 2D plane windows and outputs are mapped onto.
    pub space: Space<Window>,
    /// Handle used to stop the event loop (our exit keybind uses this).
    pub loop_signal: LoopSignal,

    // --- Smithay protocol state ---
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Vxwm>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    /// A seat groups the keyboard/pointer and tracks input focus.
    pub seat: Seat<Self>,
}

impl Vxwm {
    pub fn new(event_loop: &mut EventLoop<CalloopData>, display: Display<Self>) -> Self {
        let start_time = std::time::Instant::now();
        let dh = display.handle();

        // Register the core Wayland globals clients need.
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let popups = PopupManager::default();

        // Create a seat and advertise a keyboard + pointer. For this prototype
        // we assume both are always present (no hot-plug tracking yet).
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");
        seat.add_keyboard(Default::default(), 200, 25)
            .expect("failed to add keyboard to seat");
        seat.add_pointer();

        let space = Space::default();
        let socket_name = Self::init_wayland_listener(display, event_loop);
        let loop_signal = event_loop.get_signal();

        Self {
            start_time,
            display_handle: dh,
            space,
            loop_signal,
            socket_name,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
        }
    }

    /// Create the listening socket and wire both it and the display into the
    /// calloop event loop so client traffic is dispatched.
    fn init_wayland_listener(
        display: Display<Vxwm>,
        event_loop: &mut EventLoop<CalloopData>,
    ) -> OsString {
        // Auto-pick the next free `wayland-N` socket name.
        let listening_socket =
            ListeningSocketSource::new_auto().expect("failed to create wayland socket");
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        // New client connections: insert them into the display.
        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .expect("failed to insert client");
            })
            .expect("failed to init the wayland socket source");

        // The display itself must be polled so client requests get processed.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // SAFETY: we never drop the display while it is registered.
                    unsafe {
                        display
                            .get_mut()
                            .dispatch_clients(&mut state.state)
                            .expect("failed to dispatch wayland clients");
                    }
                    Ok(PostAction::Continue)
                },
            )
            .expect("failed to init the wayland display source");

        socket_name
    }

    /// Find the surface (and its location) under a given point. Used for
    /// pointer focus. Handy already even though we render a blank space.
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.space.element_under(pos).and_then(|(window, location)| {
            window
                .surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
                .map(|(s, p)| (s, (p + location).to_f64()))
        })
    }
}

/// Per-client data stored alongside each connected Wayland client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
