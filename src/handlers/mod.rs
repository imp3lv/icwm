//! Protocol handler trait implementations.
//!
//! Smithay dispatches client requests through a set of `*Handler` traits plus
//! `delegate_*!` macros. This module wires up the seat, data device and output
//! handlers; compositor/shm and xdg-shell live in their own submodules.

mod compositor;
mod xdg_shell;

use crate::Vxwm;

use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::selection::data_device::{
    set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
    ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::{delegate_data_device, delegate_output, delegate_seat};

//
// wl_seat
//
impl SeatHandler for Vxwm {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Vxwm> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}
delegate_seat!(Vxwm);

//
// wl_data_device (clipboard / drag-and-drop plumbing)
//
impl SelectionHandler for Vxwm {
    type SelectionUserData = ();
}

impl DataDeviceHandler for Vxwm {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for Vxwm {}
impl ServerDndGrabHandler for Vxwm {}
delegate_data_device!(Vxwm);

//
// wl_output & xdg_output
//
impl OutputHandler for Vxwm {}
delegate_output!(Vxwm);
