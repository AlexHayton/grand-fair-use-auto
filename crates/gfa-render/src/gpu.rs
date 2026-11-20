//! Device and surface setup, shared by the native and web builds.

use std::sync::Arc;

/// The wgpu instance, adapter, device and queue.
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("could not create a surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("no suitable GPU adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("could not open the GPU device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the surface is not supported by the adapter")]
    UnsupportedSurface,
}

/// A window's swapchain and its current configuration.
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

/// What happened to one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    Presented,
    /// Nothing drawn this time (timed out, reconfigured, error).
    Skipped,
    /// The window is hidden; stop drawing until it is visible again.
    Occluded,
}

impl Gpu {
    /// A new instance for whichever backends this build enables.
    pub fn instance() -> wgpu::Instance {
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
    }

    /// Create a surface for `target`, then an adapter and device that can
    /// draw to it. Async because adapter and device requests are async on
    /// the web; natively, block on it.
    pub async fn new(
        instance: wgpu::Instance,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<(Gpu, WindowSurface), GpuError> {
        let surface = instance.create_surface(target)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?}, {:?})",
            info.name,
            info.backend,
            info.device_type
        );

        let required_limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gfa"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            })
            .await?;
        device.on_uncaptured_error(Arc::new(|e: wgpu::Error| log::error!("wgpu: {e}")));

        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(GpuError::UnsupportedSurface)?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        // Vsync everywhere; the simulation runs on its own fixed tick.
        config.present_mode = wgpu::PresentMode::AutoVsync;
        log::info!(
            "surface: {:?} {}x{}",
            config.format,
            config.width,
            config.height
        );
        surface.configure(&device, &config);

        Ok((
            Gpu {
                instance,
                adapter,
                device,
                queue,
            },
            WindowSurface { surface, config },
        ))
    }
}

impl WindowSurface {
    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let max = gpu.device.limits().max_texture_dimension_2d;
        let (w, h) = (width.min(max), height.min(max));
        if (w, h) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&gpu.device, &self.config);
        log::info!("surface resized to {w}x{h}");
    }

    /// Acquire the next frame, run `draw` on it and present it.
    pub fn frame(
        &mut self,
        gpu: &Gpu,
        draw: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> FrameOutcome {
        let (texture, reconfigure) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Occluded => return FrameOutcome::Occluded,
            wgpu::CurrentSurfaceTexture::Timeout => return FrameOutcome::Skipped,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&gpu.device, &self.config);
                return FrameOutcome::Skipped;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("surface validation error");
                return FrameOutcome::Skipped;
            }
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        draw(&mut encoder, &view);
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(texture);
        if reconfigure {
            self.surface.configure(&gpu.device, &self.config);
        }
        FrameOutcome::Presented
    }
}

/// Record a pass that clears `view` to `color`.
pub fn clear(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, color: wgpu::Color) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}
