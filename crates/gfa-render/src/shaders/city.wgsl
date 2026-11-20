// The city mesh, palette-indexed.
//
// Ported from Carnage3D gamedata/shaders/city_mesh.glsl (MIT, (c) 2019
// jericho): tiles stay 8-bit and are coloured here, so remaps and animation
// are table lookups. Every read is a textureLoad, which WebGL2 runs as
// texelFetch, and no texture is filtered.

struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: eye in map pixels.
    camera: vec4<f32>,
    // x: debug view (0 normal, 1 faces by kind, 2 flat blocks).
    params: vec4<u32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
// 8-bit tiles, 32 across.
@group(0) @binding(1) var atlas: texture_2d<u32>;
// RGBA palettes, 8 per row.
@group(0) @binding(2) var palettes: texture_2d<f32>;
// Per tile: the tile it shows now (animation).
@group(0) @binding(3) var anim: texture_2d<u32>;
// Per tile: its palette under lid remaps 0..3.
@group(0) @binding(4) var tile_palettes: texture_2d<u32>;

const TABLE_WIDTH: u32 = 256u;
const TILES_PER_ROW: u32 = 32u;
const PALETTES_PER_ROW: u32 = 8u;

const TRANSPARENT: u32 = 1u;
const FLAT: u32 = 2u;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    // Texel coordinates within the tile, 0..64.
    @location(0) uv: vec2<f32>,
    // Atlas origin of the tile shown.
    @location(1) @interpolate(flat) origin: vec2<u32>,
    // x: palette; y: flags.
    @location(2) @interpolate(flat) palette_flags: vec2<u32>,
};

fn table_at(i: u32) -> vec2<i32> {
    return vec2<i32>(i32(i % TABLE_WIDTH), i32(i / TABLE_WIDTH));
}

@vertex
fn vs_main(@location(0) pos: vec4<u32>, @location(1) attr: vec4<u32>) -> VsOut {
    var world = vec3<f32>(pos.xyz);
    let flags = attr.w;
    if ((flags & FLAT) != 0u) {
        // A flat block shares its plane with a neighbour's wall; half a
        // pixel towards the eye decides which one shows.
        world += normalize(g.camera.xyz - world) * 0.5;
    }
    let tile = textureLoad(anim, table_at(pos.w), 0).r;
    let pals = textureLoad(tile_palettes, table_at(tile), 0);
    let remap = min(attr.z, 3u);
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(attr.xy);
    out.origin = vec2<u32>((tile % TILES_PER_ROW) * 64u, (tile / TILES_PER_ROW) * 64u);
    out.palette_flags = vec2<u32>(pals[remap], flags);
    return out;
}

fn debug_colour(flags: u32) -> vec3<f32> {
    let face = (flags >> 4u) & 7u;
    switch face {
        case 0u: { return vec3<f32>(0.9, 0.3, 0.3); }   // west
        case 1u: { return vec3<f32>(0.3, 0.9, 0.3); }   // east
        case 2u: { return vec3<f32>(0.3, 0.3, 0.9); }   // north
        case 3u: { return vec3<f32>(0.9, 0.9, 0.3); }   // south
        default: { return vec3<f32>(0.6, 0.6, 0.6); }   // lid
    }
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = vec2<u32>(clamp(in.uv, vec2<f32>(0.0), vec2<f32>(63.0)));
    let index = textureLoad(atlas, vec2<i32>(in.origin + t), 0).r;
    let flags = in.palette_flags.y;
    if (index == 0u && (flags & TRANSPARENT) != 0u) {
        discard;
    }
    let p = in.palette_flags.x;
    let at = vec2<i32>(i32((p % PALETTES_PER_ROW) * 256u + index), i32(p / PALETTES_PER_ROW));
    var rgb = textureLoad(palettes, at, 0).rgb;
    let mode = g.params.x;
    if (mode == 1u) {
        rgb = mix(rgb, debug_colour(flags), 0.6);
    } else if (mode == 2u && (flags & FLAT) != 0u) {
        rgb = mix(rgb, vec3<f32>(1.0, 0.0, 1.0), 0.6);
    }
    return vec4<f32>(rgb, 1.0);
}
