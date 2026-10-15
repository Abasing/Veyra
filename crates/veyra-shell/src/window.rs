//! The native window path: winit event loop + CPU-buffer presentation via
//! softbuffer.
//!
//! Boundary notes (ADR-0005/0007/0012):
//!
//! - **winit** owns window creation and event delivery. It is the native
//!   boundary explicitly sanctioned by ADR-0007 (OS API bindings through
//!   maintained, memory-safe Rust crates).
//! - **softbuffer** owns presenting our RGBA8 buffer to that window — the
//!   platform's blit path (XCB/Wayland/Win32/…), no GPU in the loop. This
//!   is the *presentation* boundary: `veyra-gfx` renders into a
//!   [`PixelBuffer`] and knows nothing about windows; this module converts
//!   the buffer to softbuffer's `0x00RRGGBB` format and hands it over.
//! - The event loop is `ControlFlow::Wait`: a diagnostic shell draws on
//!   redraw requests, not on a busy loop.
//!
//! This module is only exercised when a display server exists; headless
//! runs (CI screenshot jobs, tests) never touch it. The smoke test runs
//! it under Xvfb in CI.

use std::num::NonZeroU32;

use tracing::{debug, info};
use veyra_gfx::{PixelBuffer, RasterSurface};

use crate::clock::FrameClock;
use crate::config::SceneKind;
use crate::error::{ShellError, WindowError};
use crate::input::InputStats;
use crate::screenshot::render_scene;

/// Parameters for a window session.
#[derive(Debug, Clone)]
pub struct WindowRequest {
    /// Initial window width in pixels.
    pub width: u32,
    /// Initial window height in pixels.
    pub height: u32,
    /// Scene rendered into the window.
    pub scene: SceneKind,
    /// Window title (plain text; never claims browser functionality).
    pub title: String,
    /// Automation hook: exit the event loop cleanly after N presented
    /// frames. `None` runs until the user closes the window.
    pub exit_after_frames: Option<u64>,
}

/// Outcome of a window session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowStats {
    /// Frames actually presented to the window.
    pub frames_presented: u64,
    /// Input events accumulated during the session.
    pub input: InputStats,
    /// Logical time (microseconds) stamped for the last presented frame,
    /// from the injected clock.
    pub last_frame_micros: Option<u64>,
}

/// Runs a window session to completion (until close or the
/// `exit_after_frames` automation hook fires).
///
/// # Errors
/// [`WindowError::EventLoopUnavailable`] when no display backend exists
/// or the loop fails, [`WindowError::WindowCreation`] when the OS refuses
/// the window, presentation errors during the session (reported and the
/// loop is exited cleanly with the error preserved).
pub fn run_window(
    request: WindowRequest,
    clock: &mut dyn FrameClock,
) -> Result<WindowStats, ShellError> {
    let event_loop = winit::event_loop::EventLoop::builder()
        .build()
        .map_err(|source| ShellError::Window(WindowError::EventLoopUnavailable { source }))?;

    let mut runner = Runner {
        request,
        clock,
        input: InputStats::default(),
        frames_presented: 0,
        last_frame_micros: None,
        window: None,
        context: None,
        surface: None,
        raster: None,
        failure: None,
        done: false,
    };

    event_loop
        .run_app(&mut runner)
        .map_err(|source| ShellError::Window(WindowError::EventLoopUnavailable { source }))?;

    if let Some(failure) = runner.failure {
        return Err(failure);
    }
    Ok(WindowStats {
        frames_presented: runner.frames_presented,
        input: runner.input,
        last_frame_micros: runner.last_frame_micros,
    })
}

/// Converts the RGBA8 pixel buffer into softbuffer's presentation format:
/// one `u32` per pixel, layout `0x00RRGGBB` (the high byte must be zero;
/// alpha is dropped because the Phase 1 window surface is opaque).
pub fn rgba8_to_rgb0(buffer: &PixelBuffer) -> Vec<u32> {
    let (width, height) = (buffer.width(), buffer.height());
    let mut out = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let Some(row) = buffer.row(y) else { continue };
        for px in row.as_chunks::<{ veyra_gfx::BYTES_PER_PIXEL }>().0 {
            out.push((u32::from(px[0]) << 16) | (u32::from(px[1]) << 8) | u32::from(px[2]));
        }
    }
    out
}

struct Runner<'c> {
    request: WindowRequest,
    clock: &'c mut dyn FrameClock,
    input: InputStats,
    frames_presented: u64,
    last_frame_micros: Option<u64>,
    window: Option<std::sync::Arc<winit::window::Window>>,
    context: Option<softbuffer::Context<winit::event_loop::OwnedDisplayHandle>>,
    surface: Option<
        softbuffer::Surface<
            winit::event_loop::OwnedDisplayHandle,
            std::sync::Arc<winit::window::Window>,
        >,
    >,
    raster: Option<veyra_gfx::CpuRaster>,
    failure: Option<ShellError>,
    done: bool,
}

impl Runner<'_> {
    fn fail(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, error: ShellError) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        self.done = true;
        event_loop.exit();
    }

    /// Renders and presents one frame, then stamps it with the clock.
    ///
    /// Fallible drawing is isolated in [`Runner::draw_frame`] so no borrow
    /// of `self`'s fields is held across the failure path.
    fn present_frame(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Err(error) = self.draw_frame() {
            self.fail(event_loop, error);
            return;
        }

        // Stamp the frame with the injected clock (after a successful
        // present; the stamp is the frame's logical time).
        if let Err(error) = self.clock.advance_frame() {
            self.fail(event_loop, ShellError::Clock(error));
            return;
        }
        self.frames_presented += 1;
        self.last_frame_micros = Some(self.clock.frame_time_micros());
        debug!(
            frames = self.frames_presented,
            micros = self.last_frame_micros.unwrap_or(0),
            "frame presented"
        );

        if let Some(limit) = self.request.exit_after_frames
            && self.frames_presented >= limit
        {
            self.done = true;
            event_loop.exit();
            // The next automation frame is scheduled from
            // `about_to_wait` — see that method for why.
        }
    }

    /// Renders the scene and blits it to the window. Returns errors instead
    /// of touching the failure path, keeping field borrows local.
    fn draw_frame(&mut self) -> Result<(), ShellError> {
        let Some(raster) = self.raster.as_mut() else {
            return Ok(());
        };
        render_scene(raster, self.request.scene)?;

        let (width, height) = raster.dimensions();
        let Some(w) = NonZeroU32::new(width) else {
            return Ok(());
        };
        let Some(h) = NonZeroU32::new(height) else {
            return Ok(());
        };
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };

        // Size the presentation surface to the buffer.
        surface
            .resize(w, h)
            .map_err(|source| ShellError::Window(WindowError::Presentation { source }))?;

        // Convert RGBA8 -> 0x00RRGGBB and blit.
        let pixels = rgba8_to_rgb0(raster.buffer());
        let mut buffer = surface
            .buffer_mut()
            .map_err(|source| ShellError::Window(WindowError::Present { source }))?;
        {
            let slice: &mut [u32] = &mut buffer;
            let copy_len = slice.len().min(pixels.len());
            slice[..copy_len].copy_from_slice(&pixels[..copy_len]);
        }
        buffer
            .present()
            .map_err(|source| ShellError::Window(WindowError::Present { source }))?;
        Ok(())
    }
}

impl winit::application::ApplicationHandler for Runner<'_> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let attributes = winit::window::Window::default_attributes()
            .with_title(self.request.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.request.width,
                self.request.height,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(source) => {
                self.fail(
                    event_loop,
                    ShellError::Window(WindowError::WindowCreation { source }),
                );
                return;
            }
        };

        // Wrap in an Arc up front: softbuffer's surface stores the window
        // by value, and the runner keeps its own handle for redraws/ids.
        let window = std::sync::Arc::new(window);

        // The context holds an *owned* display handle (winit hands us one
        // explicitly) so the softbuffer types are self-contained and do not
        // borrow from the event loop.
        let context = match softbuffer::Context::new(event_loop.owned_display_handle()) {
            Ok(context) => context,
            Err(source) => {
                self.fail(
                    event_loop,
                    ShellError::Window(WindowError::Presentation { source }),
                );
                return;
            }
        };
        let surface = match softbuffer::Surface::new(&context, std::sync::Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(source) => {
                self.fail(
                    event_loop,
                    ShellError::Window(WindowError::Presentation { source }),
                );
                return;
            }
        };
        let raster = match veyra_gfx::CpuRaster::new(self.request.width, self.request.height) {
            Ok(raster) => raster,
            Err(source) => {
                self.fail(event_loop, ShellError::from(source));
                return;
            }
        };

        info!(
            width = self.request.width,
            height = self.request.height,
            scene = self.request.scene.as_str(),
            "window created; rendering the diagnostic scene"
        );
        self.window = Some(std::sync::Arc::clone(&window));
        self.context = Some(context);
        self.surface = Some(surface);
        self.raster = Some(raster);
        window.request_redraw();
    }

    /// Drives the automation hook: presents the remaining frames directly
    /// from the event loop.
    ///
    /// Why not `request_redraw()` from within the `RedrawRequested`
    /// handler? On X11, a redraw requested *during* the dispatch of a
    /// redraw can be coalesced away by winit, which stalled the smoke
    /// test after one frame. Presenting from `about_to_wait` makes the
    /// automation path independent of window-system event delivery —
    /// exactly what a headless-friendly smoke test needs. Interactive
    /// sessions (no `exit_after_frames`) are untouched: they redraw on
    /// window-system requests only.
    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.done {
            return;
        }
        if let Some(limit) = self.request.exit_after_frames
            && self.frames_presented < limit
        {
            self.present_frame(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(window) = &self.window else { return };
        if window.id() != window_id {
            return;
        }
        use winit::event::{ElementState, WindowEvent};
        match event {
            WindowEvent::CloseRequested => {
                info!("close requested; shutting down cleanly");
                self.done = true;
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if !self.done {
                    self.present_frame(event_loop);
                }
            }
            WindowEvent::Resized(size) => {
                debug!(width = size.width, height = size.height, "window resized");
                if size.width > 0 && size.height > 0 {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                let synthetic =
                    is_synthetic || matches!(event.state, ElementState::Released) && event.repeat;
                match event.state {
                    ElementState::Pressed => self.input.record_key_press(synthetic),
                    ElementState::Released => self.input.record_key_release(synthetic),
                }
                debug!(
                    synthetic,
                    state = ?event.state,
                    "keyboard input (plumbed; no content dispatch in Phase 1)"
                );
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.record_pointer_move(position.x, position.y);
                debug!(x = position.x, y = position.y, "pointer moved");
            }
            WindowEvent::MouseInput { state, .. } => {
                self.input
                    .record_pointer_button(matches!(state, ElementState::Pressed));
                debug!(?state, "pointer button");
            }
            _ => {
                // Other events (focus, occlusion, scale-factor changes, …)
                // are deliberately not interpreted in Phase 1: no behavior
                // depends on them and logging every one would be noise.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::clock::FixedFrameClock;
    use veyra_gfx::Rgba8;

    #[test]
    fn rgba8_to_rgb0_converts_and_drops_alpha() {
        let mut buffer = PixelBuffer::new(2, 1).unwrap();
        buffer
            .set_pixel(0, 0, Rgba8::new(0x12, 0x34, 0x56, 0xFF))
            .unwrap();
        // Alpha 0xAB must be dropped: the window surface is opaque.
        let (r, g, b, _a) = (0xFFu32, 0x00u32, 0x80u32, 0xABu32);
        buffer
            .set_pixel(1, 0, Rgba8::new(0xFF, 0x00, 0x80, 0xAB))
            .unwrap();
        let out = rgba8_to_rgb0(&buffer);
        assert_eq!(
            out,
            vec![(0x12 << 16) | (0x34 << 8) | 0x56, (r << 16) | (g << 8) | b,]
        );
    }

    #[test]
    fn rgba8_to_rgb0_handles_stride_padding() {
        let mut buffer = PixelBuffer::new_with_stride(1, 2, 12).unwrap();
        buffer.set_pixel(0, 0, Rgba8::opaque(1, 2, 3)).unwrap();
        buffer.set_pixel(0, 1, Rgba8::opaque(4, 5, 6)).unwrap();
        let out = rgba8_to_rgb0(&buffer);
        assert_eq!(
            out,
            vec![(1 << 16) | (2 << 8) | 3, (4 << 16) | (5 << 8) | 6]
        );
    }

    #[test]
    fn window_stats_default_is_empty() {
        // WindowStats is only produced by a real session; the struct's
        // fields are directly asserted by the Xvfb smoke test.
        let stats = WindowStats {
            frames_presented: 0,
            input: InputStats::default(),
            last_frame_micros: None,
        };
        assert_eq!(stats.frames_presented, 0);
        assert_eq!(stats.last_frame_micros, None);
    }

    #[test]
    fn fixed_clock_stamps_frames_deterministically_through_the_runner_contract() {
        // The runner stamps clock.frame_time_micros() after each
        // advance_frame(); this asserts the stamping contract the smoke
        // test relies on, without needing a display.
        let mut clock = FixedFrameClock::new(16_666).unwrap();
        let mut stamps = Vec::new();
        for _ in 0..3 {
            clock.advance_frame().unwrap();
            stamps.push(clock.frame_time_micros());
        }
        assert_eq!(stamps, vec![16_666, 33_332, 49_998]);
    }
}
