//! Rendering without a window: for `gfa-tool render`, the holecheck and
//! golden images. Native only, since it blocks on the GPU.

use crate::gpu::{Gpu, GpuError};

/// The format of offscreen images: plain RGBA, no sRGB conversion, so a
/// palette byte comes back unchanged.
pub const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Gpu {
    /// An adapter and device with no surface, under the same WebGL2 limits
    /// as the game.
    pub async fn headless() -> Result<Gpu, GpuError> {
        let instance = Gpu::instance();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                ..Default::default()
            })
            .await?;
        let required_limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gfa headless"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            })
            .await?;
        Ok(Gpu {
            instance,
            adapter,
            device,
            queue,
        })
    }
}

/// An offscreen colour target that can be read back.
pub struct Offscreen {
    texture: wgpu::Texture,
    format: wgpu::TextureFormat,
    view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
}

impl Offscreen {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        Self::with_format(device, width, height, OFFSCREEN_FORMAT)
    }

    /// A target in another 8-bit RGBA or BGRA format, e.g. a window's.
    pub fn with_format(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Offscreen {
            texture,
            format,
            view,
            width,
            height,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Copy the image back as tightly packed RGBA rows. Blocks until the GPU
    /// has finished.
    pub fn read(&self, gpu: &Gpu) -> Vec<u8> {
        let row = self.width as usize * 4;
        let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize);
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * self.height as usize) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: None,
                },
            },
            self.texture.size(),
        );
        gpu.queue.submit([encoder.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |r| {
            if let Err(e) = r {
                log::error!("readback failed: {e}");
            }
        });
        if let Err(e) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
            log::error!("waiting for the GPU: {e}");
        }
        let data = match buffer.slice(..).get_mapped_range() {
            Ok(d) => d,
            Err(e) => {
                log::error!("readback mapping: {e}");
                return vec![0; row * self.height as usize];
            }
        };
        let mut out = Vec::with_capacity(row * self.height as usize);
        for r in data.chunks(padded) {
            out.extend_from_slice(&r[..row]);
        }
        if matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for px in out.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
        }
        out
    }
}
