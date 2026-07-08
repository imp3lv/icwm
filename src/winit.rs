//! winit backend.
//!
//! Opens a window inside the current desktop session, creates a virtual
//! `Output` for it, and renders the compositor `Space` into it every frame.
//! With no windows mapped this simply clears the framebuffer to black.

use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Transform},
};

use crate::{CalloopData, Vxwm};

/// Solid black clear color (RGBA). This is what fills the window until a client
/// maps a surface.
const CLEAR_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub fn init_winit(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let display_handle = &mut data.display_handle;
    let state = &mut data.state;

    // Create the winit window + a GLES renderer bound to it.
    let (mut backend, winit) = winit::init::<GlesRenderer>()?;

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    // A virtual output representing the winit window.
    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "vxwm".into(),
            model: "winit".into(),
        },
    );
    let _global = output.create_global::<Vxwm>(display_handle);
    output.change_current_state(
        Some(mode),
        // winit framebuffers are flipped; Smithay's convention is Flipped180.
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    // Expose our socket so clients (WAYLAND_DISPLAY=...) can connect.
    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);
    tracing::info!(
        socket = ?state.socket_name,
        "winit backend ready; compositor is running"
    );

    event_loop
        .handle()
        .insert_source(winit, move |event, _, data| {
            let display = &mut data.display_handle;
            let state = &mut data.state;

            match event {
                WinitEvent::Resized { size, .. } => {
                    output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Redraw => {
                    let size = backend.window_size();
                    let damage = Rectangle::from_size(size);

                    {
                        let (renderer, mut framebuffer) = backend.bind().unwrap();
                        smithay::desktop::space::render_output::<
                            _,
                            WaylandSurfaceRenderElement<GlesRenderer>,
                            _,
                            _,
                        >(
                            &output,
                            renderer,
                            &mut framebuffer,
                            1.0,
                            0,
                            [&state.space],
                            &[],
                            &mut damage_tracker,
                            CLEAR_COLOR,
                        )
                        .unwrap();
                    }
                    backend.submit(Some(&[damage])).unwrap();

                    // Tell mapped surfaces they may draw the next frame.
                    state.space.elements().for_each(|window| {
                        window.send_frame(
                            &output,
                            state.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        )
                    });

                    state.space.refresh();
                    state.popups.cleanup();
                    let _ = display.flush_clients();

                    // Schedule the next frame.
                    backend.window().request_redraw();
                }
                WinitEvent::CloseRequested => {
                    tracing::info!("winit window closed; shutting down");
                    state.loop_signal.stop();
                }
                _ => (),
            };
        })?;

    Ok(())
}
