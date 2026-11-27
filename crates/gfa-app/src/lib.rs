//! The game shell shared by the native and web builds.
//!
//! It owns the window, the GPU surface and the frame loop. It loads a
//! level, runs the simulation at its fixed tick ([`game`]), and draws the
//! city with the player in it, followed by the camera. The backtick key shows the egui
//! debug overlay. The front end hands a selected chapter to the level loader.

mod controls;
mod game;
mod menu;
mod overlay;
#[cfg(test)]
mod popin;
mod scene;
#[cfg(target_arch = "wasm32")]
mod web;

use std::sync::Arc;

use gfa_assets::level::{self, Level};
use gfa_assets::{AssetSource, Dataset};
use gfa_render::glam::Vec3;
use gfa_render::{FrameOutcome, Gpu, WindowSurface};
use web_time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

pub use gfa_assets::DataPath;

use controls::Controls;
use game::Game;
use gfa_sim::input::Buttons;
use overlay::{Overlay, OverlayInfo, SimInfo};
use scene::Scene;

pub const TITLE: &str = "Grand Fair Use Auto";

#[derive(Debug)]
pub enum SetupAction {
    ChooseArchive,
    ChooseFolder,
    Dropped(std::path::PathBuf),
    Search,
}

#[cfg(not(target_arch = "wasm32"))]
pub type SetupHandler = fn(SetupAction) -> Result<Option<DataPath>, String>;

/// Start-up options.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// The user's game data, if found: a folder or an archive. On the web
    /// it arrives later, from the archive or folder the player selects.
    pub data: Option<DataPath>,
    /// Which game, and which mission-file section, to load.
    pub game: Dataset,
    pub level: u32,
    pub show_menu: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub setup: Option<SetupHandler>,
    /// `--nohd`: ignore HD packs even if present.
    pub no_hd: bool,
    /// Save the first frame of the city here as a PNG, then quit. Native only.
    pub screenshot: Option<std::path::PathBuf>,
    /// The window's size in logical pixels.
    pub size: (u32, u32),
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            data: None,
            game: Dataset::Gta,
            level: 1,
            show_menu: true,
            #[cfg(not(target_arch = "wasm32"))]
            setup: None,
            no_hd: false,
            screenshot: None,
            size: (1280, 800),
        }
    }
}

/// Messages to the event loop from async work.
enum UserEvent {
    /// The GPU finished initialising (async on the web).
    GpuReady(Box<Result<(Gpu, WindowSurface), gfa_render::GpuError>>),
    /// The web build's archive or folder picker delivered the game files.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    DataReady(Box<dyn AssetSource>),
    /// An on-screen button went down or up (the web page's touch buttons).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Touch(gfa_sim::input::Buttons, bool),
}

struct App {
    config: AppConfig,
    proxy: EventLoopProxy<UserEvent>,
    window: Option<Arc<Window>>,
    gfx: Option<(Gpu, WindowSurface)>,
    overlay: Option<Overlay>,
    /// Loaded before the GPU was ready; becomes a [`Scene`] when it is.
    pending: Option<Level>,
    menu: Option<menu::Menu>,
    scene: Option<Scene>,
    game: Option<Game>,
    controls: Controls,
    /// Shown in the overlay: why there is no city, for instance.
    status: String,
    started: Instant,
    last_frame: Instant,
    /// The window is hidden: draw nothing, and probe slowly in case the
    /// platform never says it is visible again.
    occluded: bool,
    stats: FrameStats,
}

impl App {
    fn new(config: AppConfig, proxy: EventLoopProxy<UserEvent>) -> Self {
        let centre = 128.0 * 64.0;
        Self {
            config,
            proxy,
            window: None,
            gfx: None,
            overlay: None,
            pending: None,
            menu: None,
            scene: None,
            game: None,
            controls: Controls::new(Vec3::new(centre, centre, 128.0)),
            status: String::new(),
            started: Instant::now(),
            last_frame: Instant::now(),
            occluded: false,
            stats: FrameStats::new(),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title(TITLE);
        // A size only for a desktop window. On the web winit would pin the
        // canvas to it with an inline style, overriding the page's
        // full-window sizing: a phone then shows only the canvas's top-left
        // corner, as if zoomed in, with the player off-screen.
        #[cfg(not(target_arch = "wasm32"))]
        let attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(
            f64::from(self.config.size.0),
            f64::from(self.config.size.1),
        ));
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

    fn needs_setup(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.config.data.is_none() && self.config.setup.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn setup_action(&mut self, action: SetupAction) {
        let Some(handler) = self.config.setup else {
            return;
        };
        match handler(action) {
            Ok(Some(data)) => match data.open() {
                Ok(src) => {
                    self.config.data = Some(data);
                    self.accept_data(src);
                }
                Err(e) => self.status = e.to_string(),
            },
            Ok(None) => {}
            Err(e) => self.status = e,
        }
    }

    fn accept_data(&mut self, src: Box<dyn AssetSource>) {
        if self.config.show_menu {
            self.game = None;
            self.scene = None;
            self.status.clear();
            #[cfg(target_arch = "wasm32")]
            web::set_playing(false);
            self.menu = Some(menu::Menu::new(src));
        } else {
            self.load_level(src.as_ref());
        }
    }

    /// Read the configured level from `src`. Errors become the overlay's
    /// status line.
    fn load_level(&mut self, src: &dyn AssetSource) {
        let started = Instant::now();
        let result = level::find(src, self.config.game, self.config.level)
            .and_then(|files| Level::load(src, files));
        match result {
            Ok(level) => {
                log::info!(
                    "level [{}] {}: {} with {} ({} ms)",
                    level.files.section,
                    level.files.name,
                    level.files.map,
                    level.style_path,
                    started.elapsed().as_millis()
                );
                if let Some(w) = &self.window {
                    w.set_title(&format!("{TITLE}: {}", level.files.name));
                }
                self.status.clear();
                self.pending = Some(level);
                self.build_scene();
            }
            Err(e) => {
                log::error!("{e}");
                self.status = e.to_string();
            }
        }
    }

    /// Turn a pending level into a scene once the GPU is up.
    fn build_scene(&mut self) {
        // Check the GPU first: taking the level when there is no GPU yet
        // would drop it.
        let Some((gpu, surface)) = self.gfx.as_ref() else {
            return;
        };
        let Some(level) = self.pending.take() else {
            return;
        };
        let started = Instant::now();
        let game = Game::new(&level);
        self.controls.jump_to(game.frame().follow.pos);
        self.game = Some(game);
        #[cfg(target_arch = "wasm32")]
        web::set_playing(true);
        self.scene = Some(Scene::new(gpu, surface.format(), surface.size(), level));
        log::info!("city uploaded in {} ms", started.elapsed().as_millis());
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// How often to retry drawing while the window is hidden.
    const OCCLUDED_PROBE: Duration = Duration::from_millis(250);

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let elapsed = now - self.last_frame;
        self.last_frame = now;
        // Paused, no time reaches the simulation, so none is owed after.
        let sim_us = if self.controls.paused {
            0
        } else {
            elapsed.as_micros() as u64
        };
        let input = self.controls.input();
        let frame = self.game.as_mut().map(|g| {
            let ticks = g.advance(sim_us, input);
            (g.frame(), g.floor_z(), ticks)
        });
        // A debug menu press is let go once a tick has run with it.
        if frame.is_some_and(|f| f.2 > 0) {
            self.controls.ticked();
        }
        let frame = frame.map(|(f, floor, _)| (f, floor));
        let dt = elapsed.as_secs_f32().min(0.1);
        // The zoom cap: no higher than shows the simulation's view, at the
        // shape the scene is drawn (pillarboxed past 21:9).
        let ceiling = match (frame, self.gfx.as_ref()) {
            (Some((f, floor)), Some((_, surface))) => {
                let (w, h) = surface.size();
                let [_, _, vw, vh] = gfa_render::camera::pillarbox(w, h);
                let aspect = vw as f32 / vh.max(1) as f32;
                Some(floor + gfa_render::camera::max_depth(f.view.0, f.view.1, aspect))
            }
            _ => None,
        };
        self.controls
            .update(dt, frame.map(|(f, _)| (f.follow.pos, f.zoom)), ceiling);

        let needs_setup = self.needs_setup();
        let Some((gpu, surface)) = self.gfx.as_mut() else {
            return;
        };
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = surface.size();
        let camera = self.controls.camera();
        let seconds = self.started.elapsed().as_secs_f64();
        let scene = &mut self.scene;
        let overlay = &mut self.overlay;
        if let (Some(scene), Some(o)) = (scene.as_mut(), overlay.as_ref()) {
            scene.city.debug_view = o.debug_view;
        }
        let inspect = match (scene.as_ref(), self.controls.cursor) {
            (Some(s), Some(c)) if overlay.as_ref().is_some_and(|o| o.visible) => {
                s.inspect(&camera, size, c.to_array())
            }
            _ => None,
        };
        let extra: Vec<_> = match (scene.as_ref(), self.game.as_ref()) {
            (Some(s), Some(g)) => s.game_sprites(g),
            _ => Vec::new(),
        };
        let sim = self.game.as_ref().map(|g| {
            let w = &g.world;
            SimInfo {
                tick: w.tick,
                cars: w.traffic.live().count(),
                peds: w.peds.count(),
                shots: w.weapons.alive(),
                fires: w.weapons.fires.iter().filter(|f| f.ticks > 0).count(),
                blasts: w.weapons.blasts.iter().filter(|b| b.frame > 0).count(),
                player: w.player_summary(),
                status: w.status_summary(),
            }
        });
        let top_up_waiting = self.controls.tapped(Buttons::ALL_WEAPONS);
        let paused = self.controls.paused;
        let frames = self.stats.total;
        let frame_ms = elapsed.as_secs_f32() * 1000.0;
        let mut actions = overlay::OverlayActions::default();
        let fps = self.stats.fps;
        let status = self.status.clone();
        let target = self.controls.target;
        let free_look = self.controls.free_look;
        let flying = self.controls.flying;
        let outcome = surface.frame(gpu, |encoder, view| {
            match scene.as_mut() {
                Some(s) => s.draw(gpu, encoder, view, size, &camera, seconds, &extra),
                None => gfa_render::clear(encoder, view, scene::SKY),
            }
            if let Some(o) = overlay.as_mut() {
                let info = OverlayInfo {
                    needs_setup,
                    fps,
                    frames,
                    frame_ms,
                    paused,
                    sim,
                    top_up_waiting,
                    level: scene.as_ref().map(|s| s.level.files.name.as_str()),
                    status: &status,
                    camera: camera.eye.to_array(),
                    target: target.to_array(),
                    free_look,
                    flying,
                    draw: scene.as_ref().map(|s| s.city.last_draw),
                    mesh: scene.as_ref().map(|s| s.city.mesh_stats),
                    inspect,
                };
                actions = o.draw(&window, gpu, encoder, view, size, &info, self.menu.as_mut());
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(action) = actions.setup {
            self.setup_action(action);
        }
        if self.menu.as_ref().is_some_and(|m| m.quit) {
            #[cfg(not(target_arch = "wasm32"))]
            event_loop.exit();
            #[cfg(target_arch = "wasm32")]
            {
                self.menu = None;
                web::show_data_picker();
            }
            return;
        }
        if let Some(section) = actions.launch
            && let Some(menu) = self.menu.take()
        {
            self.config.level = section;
            self.load_level(menu.source.as_ref());
            self.last_frame = Instant::now();
            if let (Some(scene), Some(w)) = (&self.scene, &self.window) {
                w.set_title(&format!(
                    "{TITLE} — {} — {}",
                    menu.name(),
                    scene.level.files.name
                ));
            } else {
                self.menu = Some(menu);
            }
        }
        if actions.top_up {
            self.controls.tap(Buttons::ALL_WEAPONS);
        }
        match outcome {
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
        #[cfg(not(target_arch = "wasm32"))]
        if self.scene.is_some()
            && let Some(path) = self.config.screenshot.take()
        {
            match self.screenshot(&path) {
                Ok(()) => log::info!("saved {}", path.display()),
                Err(e) => log::error!("screenshot: {e}"),
            }
            event_loop.exit();
            return;
        }
        window.request_redraw();
    }

    /// Draw the current view offscreen and save it as a PNG.
    #[cfg(not(target_arch = "wasm32"))]
    fn screenshot(&mut self, path: &std::path::Path) -> Result<(), String> {
        let (Some((gpu, surface)), Some(scene)) = (self.gfx.as_ref(), self.scene.as_mut()) else {
            return Err("nothing to draw".into());
        };
        let (w, h) = surface.size();
        let target =
            gfa_render::offscreen::Offscreen::with_format(&gpu.device, w, h, surface.format());
        let mut encoder =
            gpu.device
                .create_command_encoder(&gfa_render::wgpu::CommandEncoderDescriptor {
                    label: Some("screenshot"),
                });
        let camera = self.controls.camera();
        let seconds = self.started.elapsed().as_secs_f64();
        let extra = self
            .game
            .as_ref()
            .map(|g| scene.game_sprites(g))
            .unwrap_or_default();
        scene.draw(
            gpu,
            &mut encoder,
            target.view(),
            (w, h),
            &camera,
            seconds,
            &extra,
        );
        gpu.queue.submit([encoder.finish()]);
        let rgba = target.read(gpu);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&rgba))
            .map_err(|e| e.to_string())
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
            if self.config.no_hd {
                log::info!("HD packs disabled (--nohd)");
            }
            self.create_window(event_loop);
            match self.config.data.clone() {
                Some(d) => {
                    log::info!("game data: {} (from {})", d.root.display(), d.origin);
                    match d.open() {
                        Ok(src) => self.accept_data(src),
                        Err(e) => {
                            log::error!("{e}");
                            self.status = e.to_string();
                        }
                    }
                }
                None if self.needs_setup() => {}
                None => {
                    self.status = if cfg!(target_arch = "wasm32") {
                        "select your game archive to begin".into()
                    } else {
                        "no game data: put the game's .rar or .zip beside the program, \
                         or pass --data"
                            .into()
                    };
                    log::warn!("{}", self.status);
                }
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GpuReady(result) => match *result {
                Ok(gfx) if self.gfx.is_some() => drop(gfx),
                Ok((gpu, mut surface)) => {
                    log::debug!("GPU ready");
                    if let Some(w) = &self.window {
                        // On the web the canvas may have been laid out (and
                        // Resized delivered) while the GPU was still starting.
                        let size = w.inner_size();
                        surface.resize(&gpu, size.width, size.height);
                        self.overlay = Some(Overlay::new(w, &gpu.device, surface.format()));
                        w.request_redraw();
                    }
                    self.gfx = Some((gpu, surface));
                    self.build_scene();
                }
                Err(e) => {
                    log::error!("{e}");
                    event_loop.exit();
                }
            },
            UserEvent::DataReady(src) => self.accept_data(src),
            UserEvent::Touch(b, down) => {
                if self.menu.is_none() {
                    self.controls.touch(b, down)
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let setup = self.needs_setup();
        #[cfg(not(target_arch = "wasm32"))]
        if setup && let WindowEvent::DroppedFile(path) = &event {
            self.setup_action(SetupAction::Dropped(path.clone()));
            return;
        }
        if let (Some(o), Some(w)) = (self.overlay.as_mut(), self.window.as_ref())
            && (o.visible || setup || self.menu.is_some())
            && o.event(w, &event)
        {
            return;
        }
        if self.menu.is_some() && matches!(event, WindowEvent::KeyboardInput { .. }) {
            return;
        }
        match &event {
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
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        // Off the original's keys: its F1 to F4 were
                        // network messages (controls.rs).
                        physical_key: PhysicalKey::Code(KeyCode::Backquote),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                if let Some(o) = self.overlay.as_mut() {
                    o.visible = !o.visible;
                }
            }
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
        if !setup && self.menu.is_none() {
            self.controls.event(&event);
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

/// Measures the frame rate.
struct FrameStats {
    since: Instant,
    frames: u32,
    total: u64,
    fps: f32,
}

impl FrameStats {
    const EVERY: Duration = Duration::from_millis(500);

    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            total: 0,
            fps: 0.0,
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
            self.fps = (f64::from(self.frames) / elapsed.as_secs_f64()) as f32;
            self.since = Instant::now();
            self.frames = 0;
        }
    }
}

/// The native event loop. Make it first: on macOS it must own the
/// application object before anything else touches it, the file dialog
/// that asks for the game included.
#[cfg(not(target_arch = "wasm32"))]
pub struct NativeLoop(EventLoop<UserEvent>);

#[cfg(not(target_arch = "wasm32"))]
impl NativeLoop {
    pub fn new() -> Result<Self, winit::error::EventLoopError> {
        EventLoop::<UserEvent>::with_user_event()
            .build()
            .map(NativeLoop)
    }

    /// Run the game in a native window until it closes.
    pub fn run(self, config: AppConfig) -> Result<(), winit::error::EventLoopError> {
        let mut app = App::new(config, self.0.create_proxy());
        self.0.run_app(&mut app)
    }
}

/// Run the game in a native window until it closes.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_native(config: AppConfig) -> Result<(), winit::error::EventLoopError> {
    NativeLoop::new()?.run(config)
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
    let proxy = event_loop.create_proxy();
    web::set_data_sink(move |src| {
        let _ = proxy.send_event(UserEvent::DataReady(src));
    });
    let proxy = event_loop.create_proxy();
    web::set_touch_sink(move |b, down| {
        let _ = proxy.send_event(UserEvent::Touch(b, down));
    });
    let app = App::new(web::config_from_url(), event_loop.create_proxy());
    event_loop.spawn_app(app);
}
