// Sprites as instanced quads lying flat at their height, palette-indexed.
//
// After Carnage3D gamedata/shaders/sprites.glsl (MIT, (c) 2019 jericho).
// Colour 0 is transparent. Every read is a textureLoad.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: eye in map pixels.
    camera: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
// 8-bit sprites, shelf-packed.
@group(0) @binding(1) var atlas: texture_2d<u32>;
// RGBA palettes, 8 per row (shared with the city).
@group(0) @binding(2) var palettes: texture_2d<f32>;

const PALETTES_PER_ROW: u32 = 8u;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    // Texels from the sprite's top-left.
    @location(0) uv: vec2<f32>,
    // Atlas rectangle: x, y, width, height.
    @location(1) @interpolate(flat) rect: vec4<u32>,
    @location(2) @interpolate(flat) palette: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    // Centre in map pixels, then clockwise rotation in radians.
    @location(0) pos: vec4<f32>,
    @location(1) rect: vec4<u32>,
    @location(2) palette_flags: vec2<u32>,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    let size = vec2<f32>(rect.zw);
    // Sprite space: x right, y down (south); unrotated, the sprite points north.
    let local = (c - vec2<f32>(0.5)) * size;
    let s = sin(pos.w);
    let k = cos(pos.w);
    let turned = vec2<f32>(local.x * k - local.y * s, local.x * s + local.y * k);
    var world = vec3<f32>(pos.xy + turned, pos.z);
    // A pixel towards the eye, so a sprite on the ground wins against it.
    world += normalize(g.camera.xyz - world);
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = c * size;
    out.rect = rect;
    out.palette = palette_flags.x;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let max_t = vec2<f32>(in.rect.zw) - vec2<f32>(1.0);
    let t = vec2<u32>(clamp(in.uv, vec2<f32>(0.0), max_t));
    let index = textureLoad(atlas, vec2<i32>(in.rect.xy + t), 0).r;
    if (index == 0u) {
        discard;
    }
    let p = in.palette;
    let at = vec2<i32>(i32((p % PALETTES_PER_ROW) * 256u + index), i32(p / PALETTES_PER_ROW));
    return vec4<f32>(textureLoad(palettes, at, 0).rgb, 1.0);
}
