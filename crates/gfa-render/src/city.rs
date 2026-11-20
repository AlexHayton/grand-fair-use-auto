//! Drawing the city: the mesh from [`crate::city_mesh`], coloured by the
//! tables from [`crate::style_tables`] in `shaders/city.wgsl`.

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::camera::{Camera, Frustum};
use crate::city_mesh::{CityMesh, CityVertex, MeshStats};
use crate::style_tables::{ATLAS_WIDTH, StyleTables, TABLE_WIDTH, TileAnimator};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Debug views of the city, chosen in the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugView {
    #[default]
    Normal = 0,
    /// Tint faces by direction: west red, east green, north blue, south yellow.
    Faces = 1,
    /// Tint flat blocks magenta.
    Flat = 2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera: [f32; 4],
    params: [u32; 4],
}

/// What the last frame drew.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrawStats {
    pub chunks: usize,
    pub draws: usize,
    pub triangles: usize,
}

/// The city's GPU resources and pipeline.
pub struct CityRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    anim_texture: wgpu::Texture,
    /// The palettes, shared with the sprite renderer.
    palette_view: wgpu::TextureView,
    animator: TileAnimator,
    mesh_chunks: Vec<crate::city_mesh::Chunk>,
    /// Index ranges to draw this frame.
    runs: Vec<std::ops::Range<u32>>,
    pub mesh_stats: MeshStats,
    pub last_draw: DrawStats,
    pub debug_view: DebugView,
}

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    (width, height): (usize, usize),
    texel_bytes: usize,
    data: &[u8],
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: width as u32,
        height: height as u32,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_whole(queue, &tex, data, width * texel_bytes, size);
    tex
}

fn write_whole(
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    data: &[u8],
    bytes_per_row: usize,
    size: wgpu::Extent3d,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row as u32),
            rows_per_image: None,
        },
        size,
    );
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

impl CityRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        tables: &StyleTables,
        animator: TileAnimator,
        mesh: &CityMesh,
    ) -> Self {
        let atlas = texture(
            device,
            queue,
            "tile atlas",
            wgpu::TextureFormat::R8Uint,
            (ATLAS_WIDTH, tables.atlas_height),
            1,
            &tables.atlas,
        );
        let (pal_texels, pal_rows) = tables.palette_texels();
        let palettes = texture(
            device,
            queue,
            "palettes",
            wgpu::TextureFormat::Rgba8Unorm,
            (ATLAS_WIDTH, pal_rows),
            4,
            &pal_texels,
        );
        let (tp_texels, tp_rows) = tables.tile_palette_texels();
        let tile_palettes = texture(
            device,
            queue,
            "tile palettes",
            wgpu::TextureFormat::Rgba16Uint,
            (TABLE_WIDTH, tp_rows),
            8,
            bytemuck::cast_slice(&tp_texels),
        );
        let anim_texture = texture(
            device,
            queue,
            "tile animation",
            wgpu::TextureFormat::R16Uint,
            (TABLE_WIDTH, animator.rows()),
            2,
            bytemuck::cast_slice(animator.texels()),
        );

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("city globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uint = wgpu::TextureSampleType::Uint;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("city"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(1, uint),
                texture_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
                texture_entry(3, uint),
                texture_entry(4, uint),
            ],
        });
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let (atlas_v, pal_v, anim_v, tp_v) = (
            view(&atlas),
            view(&palettes),
            view(&anim_texture),
            view(&tile_palettes),
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("city"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_v),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&pal_v),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&anim_v),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&tp_v),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("city.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/city.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("city"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint16x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint8x4,
                offset: 8,
                shader_location: 1,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("city"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CityVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // Faces are wound clockwise as seen from their front.
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(target_format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        // WebGL2 has no base-vertex draws, so indices are absolute (u32).
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("city vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("city indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        CityRenderer {
            pipeline,
            bind_group,
            globals,
            vertices,
            indices,
            anim_texture,
            palette_view: pal_v,
            animator,
            mesh_chunks: mesh.chunks.clone(),
            runs: Vec::new(),
            mesh_stats: mesh.stats,
            last_draw: DrawStats::default(),
            debug_view: DebugView::Normal,
        }
    }

    /// Advance tile animations to `seconds` of render time.
    pub fn animate(&mut self, queue: &wgpu::Queue, seconds: f64) {
        let frame = (seconds * crate::style_tables::ORIGINAL_FRAME_HZ) as u64;
        if self.animator.set_frame(frame) {
            let size = self.anim_texture.size();
            write_whole(
                queue,
                &self.anim_texture,
                bytemuck::cast_slice(self.animator.texels()),
                TABLE_WIDTH * 2,
                size,
            );
        }
    }

    /// Upload this frame's camera. Call before [`CityRenderer::draw`].
    pub fn prepare(&mut self, queue: &wgpu::Queue, camera: &Camera, aspect: f32) {
        let vp = camera.view_proj(aspect);
        let g = Globals {
            view_proj: vp.to_cols_array_2d(),
            camera: camera.eye.extend(1.0).to_array(),
            params: [self.debug_view as u32, 0, 0, 0],
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));

        // Visible chunks, merged into runs: chunks are contiguous in the
        // index buffer, so neighbours in a row become one draw.
        let frustum = Frustum::new(&vp);
        self.runs.clear();
        let mut stats = DrawStats::default();
        for c in &self.mesh_chunks {
            if c.index_count == 0 {
                continue;
            }
            let (min, max) = c.bounds();
            if !frustum.intersects_box(Vec3::from(min), Vec3::from(max)) {
                continue;
            }
            stats.chunks += 1;
            stats.triangles += c.index_count as usize / 3;
            let range = c.first_index..c.first_index + c.index_count;
            match self.runs.last_mut() {
                Some(last) if last.end == range.start => last.end = range.end,
                _ => self.runs.push(range),
            }
        }
        stats.draws = self.runs.len();
        self.last_draw = stats;
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        for r in &self.runs {
            pass.draw_indexed(r.clone(), 0, 0..1);
        }
    }

    /// The palette texture, for other renderers that colour with it.
    pub fn palette_view(&self) -> &wgpu::TextureView {
        &self.palette_view
    }

    pub fn animations(&self) -> usize {
        self.animator.len()
    }
}

/// A depth buffer sized to a render target.
pub struct DepthTarget {
    view: wgpu::TextureView,
    size: (u32, u32),
}

impl DepthTarget {
    pub fn new(device: &wgpu::Device, (width, height): (u32, u32)) -> Self {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        DepthTarget {
            view: tex.create_view(&wgpu::TextureViewDescriptor::default()),
            size: (width, height),
        }
    }

    /// Recreate if the target changed size.
    pub fn fit(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        if self.size != size {
            *self = DepthTarget::new(device, size);
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

/// Begin a pass that clears colour and depth.
pub fn begin_scene<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &DepthTarget,
    clear: wgpu::Color,
) -> wgpu::RenderPass<'e> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("scene"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth.view(),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Discard,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    })
}
