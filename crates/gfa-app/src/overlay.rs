//! The egui debug overlay: frame rate, camera, draw counts and a block
//! inspector. F1 shows and hides it.

use std::sync::Arc;

use gfa_render::city::DebugView;
use winit::window::Window;

/// What the overlay shows this frame.
pub struct OverlayInfo<'a> {
    pub fps: f32,
    pub level: Option<&'a str>,
    pub status: &'a str,
    pub camera: [f32; 3],
    pub target: [f32; 3],
    pub free_look: bool,
    pub flying: bool,
    pub player: Option<&'a str>,
    pub draw: Option<gfa_render::city::DrawStats>,
    pub mesh: Option<gfa_render::city_mesh::MeshStats>,
    pub inspect: Option<String>,
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
            visible: true,
            debug_view: DebugView::Normal,
        }
    }

    /// Feed a window event. Returns whether egui wants it to itself.
    pub fn event(&mut self, window: &Window, event: &winit::event::WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }

    /// Build the overlay and draw it over `view`.
    pub fn draw(
        &mut self,
        window: &Window,
        gpu: &gfa_render::Gpu,
        encoder: &mut gfa_render::wgpu::CommandEncoder,
        view: &gfa_render::wgpu::TextureView,
        size: (u32, u32),
        info: &OverlayInfo<'_>,
    ) {
        if !self.visible {
            return;
        }
        let (device, queue) = (&gpu.device, &gpu.queue);
        let input = self.state.take_egui_input(window);
        let mut debug_view = self.debug_view;
        let output = self.ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            egui::Window::new("debug")
                .default_pos([12.0, 12.0])
                .resizable(false)
                .show(&ctx, |ui| panel(ui, info, &mut debug_view));
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
    }
}

fn panel(ui: &mut egui::Ui, info: &OverlayInfo<'_>, debug_view: &mut DebugView) {
    ui.label(format!("{:.0} fps", info.fps));
    ui.label(info.level.unwrap_or("no level loaded"));
    if !info.status.is_empty() {
        ui.colored_label(egui::Color32::YELLOW, info.status);
    }
    ui.separator();
    let blocks =
        |p: [f32; 3]| format!("{:.2}, {:.2}, {:.2}", p[0] / 64.0, p[1] / 64.0, p[2] / 64.0);
    if let Some(p) = info.player {
        ui.label(p);
    }
    ui.label(format!("camera {}", blocks(info.camera)));
    ui.label(format!("target {}", blocks(info.target)));
    ui.label(match (info.flying, info.free_look) {
        (_, true) => "free look: right-drag to tilt and turn, R resets",
        (true, false) => "flying: arrows/WASD pan, C returns to the player",
        (false, false) => "arrows/WASD run, Shift walks, wheel zooms, F free look, C fly",
    });
    if let Some(d) = info.draw {
        ui.label(format!(
            "{} chunks in {} draws, {} triangles",
            d.chunks, d.draws, d.triangles
        ));
    }
    if let Some(m) = info.mesh {
        ui.label(format!(
            "map: {} faces, {} flat, {} sloped blocks",
            m.faces, m.flat_blocks, m.sloped_blocks
        ));
    }
    ui.horizontal(|ui| {
        ui.label("view");
        ui.selectable_value(debug_view, DebugView::Normal, "normal");
        ui.selectable_value(debug_view, DebugView::Faces, "faces");
        ui.selectable_value(debug_view, DebugView::Flat, "flat");
    });
    if let Some(text) = &info.inspect {
        ui.separator();
        ui.monospace(text);
    }
}
