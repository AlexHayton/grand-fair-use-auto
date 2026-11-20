//! The game shell shared by the native and web builds.
//!
//! It owns the window, the GPU surface and the frame loop. Game states (boot,
//! front end, briefing, game), input mapping and the debug overlay come in
//! later milestones. M0 opens a window and clears it every frame.

use std::sync::Arc;

use gfa_render::{FrameOutcome, Gpu, WindowSurface, wgpu};
use web_time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub use gfa_assets::DataPath;

pub const TITLE: &str = "Grand Fair Use Auto";

/// Clear colour until there is a city to draw.
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.05,
    g: 0.06,
    b: 0.10,
    a: 1.0,
};

/// Start-up options.
#[derive(Debug, Default, Clone)]
pub struct AppConfig {
    /// The user's game data, if found. On the web it arrives later from the
    /// folder picker.
    pub data: Option<DataPath>,
    /// `--nohd`: ignore HD packs even if present.
    pub no_hd: bool,
}

/// Messages to the event loop from async work.
enum UserEvent {
    /// The GPU finished initialising (async on the web).
    GpuReady(Box<Result<(Gpu, WindowSurface), gfa_render::GpuError>>),
}

struct App {
    config: AppConfig,
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    gfx: Option<(Gpu, WindowSurface)>,
    /// The window is hidden: draw nothing, and probe slowly in case the
    /// platform never says it is visible again.
    occluded: bool,
    stats: FrameStats,
}

impl App {
    fn new(config: AppConfig, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            config,
            proxy,
            window: None,
            gfx: None,
            occluded: false,
            stats: FrameStats::new(),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title(TITLE)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        #[cfg(target_arch = "wasm32")]
        let attrs = web::attach_canvas(attrs);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("could not create the window: {e}");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        let init = Gpu::new(Gpu::instance(), window.clone(), size.width, size.height);
        self.window = Some(window);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let result = pollster::block_on(init);
            let _ = self.proxy.send_event(UserEvent::GpuReady(Box::new(result)));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = proxy.send_event(UserEvent::GpuReady(Box::new(init.await)));
            });
        }
    }

    /// How often to retry drawing while the window is hidden.
    const OCCLUDED_PROBE: Duration = Duration::from_millis(250);

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some((gpu, surface)) = self.gfx.as_mut() else {
            return;
        };
        match surface.frame(gpu, |encoder, view| gfa_render::clear(encoder, view, CLEAR)) {
            FrameOutcome::Presented => self.stats.frame(),
            FrameOutcome::Skipped => {}
            FrameOutcome::Occluded => {
                if !self.occluded {
                    log::debug!("window hidden; pausing drawing");
                }
                self.set_occluded(event_loop, true);
                return;
            }
        }
        self.set_occluded(event_loop, false);
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn set_occluded(&mut self, event_loop: &ActiveEventLoop, occluded: bool) {
        self.occluded = occluded;
        event_loop.set_control_flow(if occluded {
            ControlFlow::WaitUntil(Instant::now() + Self::OCCLUDED_PROBE)
        } else {
            ControlFlow::Wait
        });
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            match &self.config.data {
                Some(d) => log::info!("game data: {} (from {})", d.root.display(), d.origin),
                None => log::warn!("no game data yet; showing an empty screen"),
            }
            if self.config.no_hd {
                log::info!("HD packs disabled (--nohd)");
            }
            self.create_window(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GpuReady(result) => match *result {
                Ok(gfx) if self.gfx.is_some() => drop(gfx),
                Ok((gpu, mut surface)) => {
                    log::debug!("GPU ready");
                    // On the web the canvas may have been laid out (and
                    // Resized delivered) while the GPU was still starting.
                    if let Some(w) = &self.window {
                        let size = w.inner_size();
                        surface.resize(&gpu, size.width, size.height);
                        w.request_redraw();
                    }
                    self.gfx = Some((gpu, surface));
                }
                Err(e) => {
                    log::error!("{e}");
                    event_loop.exit();
                }
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } if cfg!(not(target_arch = "wasm32")) => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some((gpu, surface)) = self.gfx.as_mut() {
                    surface.resize(gpu, size.width, size.height);
                }
            }
            WindowEvent::Occluded(false) if self.occluded => {
                self.set_occluded(event_loop, false);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // While hidden, the loop wakes every OCCLUDED_PROBE to try again.
        if self.occluded
            && let Some(w) = &self.window
        {
            w.request_redraw();
        }
    }
}

/// Logs the frame rate every few seconds.
struct FrameStats {
    since: Instant,
    frames: u32,
    total: u64,
}

impl FrameStats {
    const EVERY: Duration = Duration::from_secs(5);

    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            total: 0,
        }
    }

    fn frame(&mut self) {
        if self.total == 0 {
            log::info!("first frame presented");
        }
        self.frames += 1;
        self.total += 1;
        let elapsed = self.since.elapsed();
        if elapsed >= Self::EVERY {
            log::debug!("{:.1} fps", f64::from(self.frames) / elapsed.as_secs_f64());
            self.since = Instant::now();
            self.frames = 0;
        }
    }
}

/// Run the game in a native window until it closes.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native(config: AppConfig) -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App::new(config, event_loop.create_proxy());
    event_loop.run_app(&mut app)
}

/// Start the game in the page's `<canvas id="{canvas_id}">`. Returns at once;
/// the browser drives the loop.
#[cfg(target_arch = "wasm32")]
pub fn run_web(canvas_id: &str) {
    use winit::platform::web::EventLoopExtWebSys;
    web::set_canvas_id(canvas_id);
    let event_loop = match EventLoop::<UserEvent>::with_user_event().build() {
        Ok(l) => l,
        Err(e) => {
            log::error!("could not start the event loop: {e}");
            return;
        }
    };
    let app = App::new(AppConfig::default(), event_loop.create_proxy());
    event_loop.spawn_app(app);
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowAttributesExtWebSys;
    use winit::window::WindowAttributes;

    thread_local! {
        static CANVAS_ID: RefCell<String> = RefCell::new(String::from("gfa-canvas"));
    }

    pub fn set_canvas_id(id: &str) {
        CANVAS_ID.with(|c| *c.borrow_mut() = id.to_string());
    }

    /// Draw into the page's canvas if it has one, else let winit append one.
    pub fn attach_canvas(attrs: WindowAttributes) -> WindowAttributes {
        let id = CANVAS_ID.with(|c| c.borrow().clone());
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        match canvas {
            Some(c) => attrs
                .with_canvas(Some(c))
                .with_prevent_default(true)
                .with_focusable(true),
            None => attrs.with_append(true),
        }
    }
}
