//! The egui debug menu: the frame counter and frame rate, the simulation's
//! and the renderer's counts, the player's state with an ammunition
//! top-up, the camera, the view mode and a block inspector. The backtick
//! key (`` ` ``) shows and hides it.

use std::sync::Arc;

use gfa_render::city::DebugView;
use winit::window::Window;

/// The simulation's counts for the menu.
pub struct SimInfo {
    pub tick: u32,
    pub cars: usize,
    pub peds: usize,
    pub shots: usize,
    pub fires: usize,
    pub blasts: usize,
    /// Where he is and what he is doing.
    pub player: String,
    /// Health, gun, score and wanted level.
    pub status: String,
}

/// What the overlay shows this frame.
pub struct OverlayInfo<'a> {
    pub needs_setup: bool,
    pub fps: f32,
    /// Frames presented since the start.
    pub frames: u64,
    /// The last frame's time, milliseconds.
    pub frame_ms: f32,
    pub paused: bool,
    pub sim: Option<SimInfo>,
    /// The top-up is pressed and waiting for a tick (none run while paused).
    pub top_up_waiting: bool,
    pub level: Option<&'a str>,
    pub status: &'a str,
    pub camera: [f32; 3],
    pub target: [f32; 3],
    pub free_look: bool,
    pub flying: bool,
    pub draw: Option<gfa_render::city::DrawStats>,
    pub mesh: Option<gfa_render::city_mesh::MeshStats>,
    pub inspect: Option<String>,
}

/// What the menu's buttons asked for this frame.
#[derive(Debug, Default)]
pub struct OverlayActions {
    /// Every gun with 99 rounds (keypad `*`).
    pub top_up: bool,
    pub launch: Option<u32>,
    pub setup: Option<crate::SetupAction>,
}

pub struct Overlay {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    pub visible: bool,
    pub debug_view: DebugView,
}

impl Overlay {
    pub fn new(
        window: &Arc<Window>,
        device: &gfa_render::wgpu::Device,
        format: gfa_render::wgpu::TextureFormat,
    ) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(
            device,
            format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: false,
                ..Default::default()
            },
        );
        Overlay {
            ctx,
            state,
            renderer,
            visible: false,
            debug_view: DebugView::Normal,
        }
    }

    /// Feed a window event. Returns whether egui wants it to itself.
    pub fn event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    /// Build the overlay and draw it over `view`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        window: &Window,
        gpu: &gfa_render::Gpu,
        encoder: &mut gfa_render::wgpu::CommandEncoder,
        view: &gfa_render::wgpu::TextureView,
        size: (u32, u32),
        info: &OverlayInfo<'_>,
        mut menu: Option<&mut crate::menu::Menu>,
    ) -> OverlayActions {
        let mut actions = OverlayActions::default();
        if !self.visible && menu.is_none() && !info.needs_setup {
            return actions;
        }
        let (device, queue) = (&gpu.device, &gpu.queue);
        let input = self.state.take_egui_input(window);
        let mut debug_view = self.debug_view;
        let output = self.ctx.run_ui(input, |ui| {
            if info.needs_setup {
                egui::Window::new("Choose your game data")
                    .anchor(egui::Align2::CENTER_CENTER, [0., 0.])
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        ui.group(|ui| {
                            ui.add_space(14.);
                            ui.label("Drop a .zip or .rar archive here from Downloads");
                            ui.add_space(14.);
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Choose archive…").clicked() {
                                actions.setup = Some(crate::SetupAction::ChooseArchive);
                            }
                            if ui.button("Choose folder…").clicked() {
                                actions.setup = Some(crate::SetupAction::ChooseFolder);
                            }
                        });
                        if ui.link("Search Google for GTA 1 Directors Cut").clicked() {
                            actions.setup = Some(crate::SetupAction::Search);
                        }
                        if !info.status.is_empty() {
                            ui.label(info.status);
                        }
                    });
                return;
            }
            if let Some(menu) = menu.as_deref_mut() {
                actions.launch = menu.draw(ui, info.status);
                return;
            }
            let ctx = ui.ctx().clone();
            egui::Window::new("debug")
                .default_pos([12.0, 12.0])
                .resizable(false)
                .show(&ctx, |ui| panel(ui, info, &mut debug_view, &mut actions));
        });
        self.debug_view = debug_view;
        self.state
            .handle_platform_output(window, output.platform_output);

        let jobs = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.0, size.1],
            pixels_per_point: output.pixels_per_point,
        };
        // epaint insists every delta is consumed, so drain them.
        let mut textures = output.textures_delta;
        for (id, deltas) in textures.set.drain() {
            for delta in &deltas {
                self.renderer.update_texture(device, queue, id, delta);
            }
        }
        let extra = self
            .renderer
            .update_buffers(device, queue, encoder, &jobs, &screen);
        if !extra.is_empty() {
            queue.submit(extra);
        }
        {
            let pass = encoder.begin_render_pass(&gfa_render::wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(gfa_render::wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: gfa_render::wgpu::Operations {
                        load: gfa_render::wgpu::LoadOp::Load,
                        store: gfa_render::wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        for id in textures.free.drain() {
            self.renderer.free_texture(&id);
        }
        actions
    }
}

fn panel(
    ui: &mut egui::Ui,
    info: &OverlayInfo<'_>,
    debug_view: &mut DebugView,
    actions: &mut OverlayActions,
) {
    // The counter line: frames drawn, the simulation's tick, the rate.
    let tick = info
        .sim
        .as_ref()
        .map_or(String::new(), |s| format!("  tick {}", s.tick));
    ui.monospace(format!(
        "frame {}{tick}  {:.0} fps  {:.1} ms{}",
        info.frames,
        info.fps,
        info.frame_ms,
        if info.paused { "  PAUSED" } else { "" }
    ));
    ui.label(info.level.unwrap_or("no level loaded"));
    if !info.status.is_empty() {
        ui.colored_label(egui::Color32::YELLOW, info.status);
    }

    if let Some(sim) = &info.sim {
        ui.separator();
        ui.label(&sim.player);
        ui.label(&sim.status);
        ui.horizontal(|ui| {
            let button = ui
                .button("Top up ammo")
                .on_hover_text("Every gun with 99 rounds, as keypad * does");
            if button.clicked() {
                actions.top_up = true;
            }
            if info.top_up_waiting {
                ui.weak("waiting for a tick");
            }
        });
    }

    egui::CollapsingHeader::new("stats")
        .default_open(true)
        .show(ui, |ui| {
            if let Some(sim) = &info.sim {
                ui.label(format!(
                    "sim: {} cars, {} people, {} shots, {} fires, {} explosions",
                    sim.cars, sim.peds, sim.shots, sim.fires, sim.blasts
                ));
            }
            if let Some(d) = info.draw {
                ui.label(format!(
                    "draw: {} chunks in {} draws, {} triangles",
                    d.chunks, d.draws, d.triangles
                ));
            }
            if let Some(m) = info.mesh {
                ui.label(format!(
                    "map: {} faces, {} flat, {} sloped blocks",
                    m.faces, m.flat_blocks, m.sloped_blocks
                ));
            }
        });

    egui::CollapsingHeader::new("camera")
        .default_open(false)
        .show(ui, |ui| {
            let blocks =
                |p: [f32; 3]| format!("{:.2}, {:.2}, {:.2}", p[0] / 64.0, p[1] / 64.0, p[2] / 64.0);
            ui.label(format!("camera {}", blocks(info.camera)));
            ui.label(format!("target {}", blocks(info.target)));
            ui.label(match (info.flying, info.free_look) {
                (_, true) => "free look: right-drag to tilt and turn, R resets",
                (true, false) => "flying: arrows/WASD pan, C returns to the player",
                (false, false) => "arrows/WASD run, Shift walks, wheel zooms, F free look, C fly",
            });
            ui.horizontal(|ui| {
                ui.label("view");
                ui.selectable_value(debug_view, DebugView::Normal, "normal");
                ui.selectable_value(debug_view, DebugView::Faces, "faces");
                ui.selectable_value(debug_view, DebugView::Flat, "flat");
            });
        });

    if let Some(text) = &info.inspect {
        ui.separator();
        ui.monospace(text);
    }
}
