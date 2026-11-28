// 2D HUD quad batcher: fonts, icons, drop shadows, and per-quad clipping
// (for the scrolling pager window). Drawn in virtual HUD coordinates
// (480 px tall, aspect-matched to the game viewport).

struct Globals {
    // xy: virtual HUD width and height.
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) hud_pos: vec2<f32>,
    @location(2) @interpolate(flat) tint: vec4<f32>,
    @location(3) @interpolate(flat) clip_rect: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    // Virtual HUD destination rectangle: x, y, w, h (top-left origin).
    @location(0) dst: vec4<f32>,
    // Normalized atlas UV rectangle: u0, v0, u1, v1.
    @location(1) src: vec4<f32>,
    // RGBA tint; negative alpha signals flat-shadow mode (rgb = 0, alpha = -tint.a * tex.a).
    @location(2) tint: vec4<f32>,
    // Virtual HUD clip rectangle: x0, y0, x1, y1.
    @location(3) clip_rect: vec4<f32>,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    let pos = dst.xy + c * dst.zw;
    let ndc = vec2<f32>(
        (pos.x / g.screen.x) * 2.0 - 1.0,
        1.0 - (pos.y / g.screen.y) * 2.0,
    );
    var out: VsOut;
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = mix(src.xy, src.zw, c);
    out.hud_pos = pos;
    out.tint = tint;
    out.clip_rect = clip_rect;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    if (in.hud_pos.x < in.clip_rect.x || in.hud_pos.x > in.clip_rect.z ||
        in.hud_pos.y < in.clip_rect.y || in.hud_pos.y > in.clip_rect.w) {
        discard;
    }
    let tex = textureSample(atlas, atlas_sampler, in.uv);
    if (tex.a <= 0.01) {
        discard;
    }
    if (in.tint.a < 0.0) {
        return vec4<f32>(in.tint.rgb, tex.a * (-in.tint.a));
    }
    return tex * in.tint;
}
